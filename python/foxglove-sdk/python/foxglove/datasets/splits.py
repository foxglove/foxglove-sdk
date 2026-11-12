"""Reproducible episode pools for dataset readers."""

from __future__ import annotations

import hashlib
import json
import math
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Literal, cast

SplitName = Literal["train", "validation", "test"]


@dataclass(frozen=True)
class SplitSummary:
    """Metadata counts for one split, before rank or worker partitioning."""

    group_count: int
    """Number of connected recording groups in the split, including custom grouping
    constraints."""

    episode_count: int
    """Number of episodes in the split."""


@dataclass(frozen=True)
class DatasetSplit:
    """Assign whole recording groups to train, validation, and test sets.

    Episodes connected by a shared recording form one group. Each group is
    assigned according to a hash of its smallest custom group ID, or its smallest
    recording ID when ``groups`` is omitted, together with ``seed`` and the
    configured probabilities. Membership is reproducible for a committed dataset
    version and configuration, independent of API order, framework, source, epoch,
    ranks, and workers.

    :param train: Optional. Probability that a group is assigned to the training
        set. The three proportions must be finite, nonnegative, and sum to one.
    :param validation: Optional. Probability that a group is assigned to the
        validation set.
    :param test: Optional. Probability that a group is assigned to the test set.
    :param seed: Optional. Seed for the assignment hash. Independent of epoch
        shuffling.
    :param groups: Optional. Maps every episode ID to a session, site, or other
        group ID, and must cover exactly the episodes of the committed version.
        These constraints are added to recording grouping, so shared recordings
        always stay together. The mapping is snapshotted as sorted, immutable
        episode/group pairs; these pairs are also accepted as sequences when
        restoring from ``dataclasses.asdict`` or JSON.
    :raises ValueError: If a proportion is not a finite, nonnegative number, the
        proportions do not sum to one, ``seed`` is not an integer, or ``groups``
        is not a mapping or sequence of episode/group pairs of nonempty strings
        with unique episode IDs.

    Example::

        from foxglove.datasets import DatasetSplit, inspect_splits

        split = DatasetSplit(train=0.8, validation=0.1, test=0.1, seed=0)
        summaries = inspect_splits("dset_123", version=2, split_config=split)
    """

    train: float = 0.8
    validation: float = 0.1
    test: float = 0.1
    seed: int = 0
    groups: Mapping[str, str] | Sequence[Sequence[str]] | None = None

    def __post_init__(self) -> None:
        groups = self.groups
        proportions = (self.train, self.validation, self.test)
        if any(
            isinstance(value, bool)
            or not isinstance(value, (int, float))
            or not math.isfinite(value)
            or value < 0
            for value in proportions
        ) or not math.isclose(sum(proportions), 1.0, rel_tol=0, abs_tol=1e-12):
            raise ValueError(
                "Split proportions must be finite, nonnegative, and sum to one"
            )
        if isinstance(self.seed, bool) or not isinstance(self.seed, int):
            raise ValueError("Split seed must be an integer")
        if groups is not None:
            pairs = tuple(groups.items()) if isinstance(groups, Mapping) else groups
            if (
                not isinstance(pairs, Sequence)
                or isinstance(pairs, (str, bytes, bytearray))
                or any(
                    not isinstance(pair, Sequence)
                    or isinstance(pair, (str, bytes, bytearray))
                    or len(pair) != 2
                    or any(not isinstance(value, str) or not value for value in pair)
                    for pair in pairs
                )
            ):
                raise ValueError(
                    "Split groups must be a mapping or sequence of episode/group pairs "
                    "with nonempty string IDs"
                )
            snapshot = tuple(sorted((pair[0], pair[1]) for pair in pairs))
            if len(dict(snapshot)) != len(snapshot):
                raise ValueError("Split groups must contain unique episode IDs")
            object.__setattr__(self, "groups", snapshot)

    def _select(
        self, recordings: Mapping[str, Sequence[str]], name: SplitName
    ) -> set[str]:
        return {
            episode_id
            for episodes in self._partition(recordings)[name].values()
            for episode_id in episodes
        }

    def _partition(
        self, recordings: Mapping[str, Sequence[str]]
    ) -> dict[SplitName, dict[str, list[str]]]:
        parents = {episode_id: episode_id for episode_id in recordings}

        def root(episode_id: str) -> str:
            current = episode_id
            while parents[current] != current:
                parents[current] = parents[parents[current]]
                current = parents[current]
            return current

        def join(first: str, second: str) -> None:
            left, right = sorted((root(first), root(second)))
            parents[right] = left

        owners: dict[str, str] = {}
        for episode_id, recording_ids in recordings.items():
            for recording_id in recording_ids:
                join(episode_id, owners.setdefault(recording_id, episode_id))
        groups = (
            dict(cast(tuple[tuple[str, str], ...], self.groups))
            if self.groups is not None
            else None
        )
        if groups is not None:
            if groups.keys() != recordings.keys():
                raise ValueError(
                    "Split groups must cover exactly the committed version's episodes"
                )
            group_owners: dict[str, str] = {}
            for episode_id, group in groups.items():
                join(episode_id, group_owners.setdefault(group, episode_id))
        keys: dict[str, str] = {}
        for episode_id, recording_ids in recordings.items():
            group = root(episode_id)
            candidates = recording_ids if groups is None else (groups[episode_id],)
            keys[group] = min(keys.get(group, candidates[0]), *candidates)
        assignments: dict[str, SplitName] = {}
        for episode_id in recordings:
            group = root(episode_id)
            if group not in assignments:
                payload = json.dumps(
                    ["foxglove-dataset-split-v1", self.seed, keys[group]],
                    separators=(",", ":"),
                ).encode()
                value = int.from_bytes(hashlib.sha256(payload).digest(), "big")
                scale = 1 << 256
                assignments[group] = (
                    "train"
                    if value < self.train * scale
                    else (
                        "validation"
                        if value < (self.train + self.validation) * scale
                        else "test"
                    )
                )
        partitions: dict[SplitName, dict[str, list[str]]] = {
            "train": {},
            "validation": {},
            "test": {},
        }
        for episode_id in recordings:
            group = root(episode_id)
            partitions[assignments[group]].setdefault(group, []).append(episode_id)
        return partitions
