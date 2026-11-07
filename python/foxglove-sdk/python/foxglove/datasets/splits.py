"""Reproducible episode pools for dataset readers."""

from __future__ import annotations

import hashlib
import json
import math
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Literal

SplitName = Literal["train", "validation", "test"]


@dataclass(frozen=True)
class SplitSummary:
    """Metadata counts for one split, before rank or worker partitioning.

    ``group_count`` counts connected recording groups, including custom grouping
    constraints. ``episode_count`` counts their episodes. Neither count measures
    samples or frames, which depend on decoding and the episode callback.
    """

    group_count: int
    episode_count: int


@dataclass(frozen=True)
class DatasetSplit:
    """Assign whole recording groups to train, validation, and test sets.

    Proportions must be finite, nonnegative, and sum to one. Each group's hash
    selects a split with these probabilities; counts are not exact, and small
    sets can be empty. Use :func:`foxglove.datasets.inspect_splits` to check
    group and episode counts before training or evaluation.
    ``seed`` is independent of epoch shuffling. ``groups`` optionally maps every
    episode ID to a session, site, or other group ID. These constraints are added
    to recording grouping, so shared recordings always stay together. The mapping
    is snapshotted as sorted, immutable episode/group pairs; these pairs are also
    accepted when restoring a configuration from ``dataclasses.asdict``.

    Membership is reproducible for a committed version and this configuration,
    independent of API order, framework, source, epoch, ranks, and workers.
    Each group is hashed by its smallest recording ID. Adding episodes within
    existing recording groups preserves membership. Merging groups or changing
    their smallest recording ID in another version can change assignments. Save
    the version and split configuration with model artifacts.
    """

    train: float = 0.8
    validation: float = 0.1
    test: float = 0.1
    seed: int = 0
    groups: Mapping[str, str] | tuple[tuple[str, str], ...] | None = None

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
            if not isinstance(pairs, tuple) or any(
                not isinstance(pair, tuple)
                or len(pair) != 2
                or any(not isinstance(value, str) or not value for value in pair)
                for pair in pairs
            ):
                raise ValueError(
                    "Split groups must map nonempty episode IDs to nonempty group IDs"
                )
            if len(dict(pairs)) != len(pairs):
                raise ValueError("Split groups must contain unique episode IDs")
            object.__setattr__(self, "groups", tuple(sorted(pairs)))

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
        if self.groups is not None:
            groups = dict(self.groups)
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
            keys[group] = min(keys.get(group, recording_ids[0]), *recording_ids)
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
