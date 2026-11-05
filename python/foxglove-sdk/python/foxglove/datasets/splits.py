"""Reproducible episode pools for dataset readers."""

from __future__ import annotations

import hashlib
import json
import math
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from types import MappingProxyType
from typing import Literal

SplitName = Literal["train", "validate", "eval"]


@dataclass(frozen=True)
class DatasetSplit:
    """Assign whole recording groups to train, validation, and final evaluation.

    Proportions must be finite, nonnegative, and sum to one. Hash assignment
    approximates these proportions by group count; small pools can be empty.
    ``seed`` is independent of epoch shuffling. ``groups`` optionally maps every
    episode ID to a session, site, or other group ID. These constraints are added
    to recording grouping, so shared recordings always stay together.

    Membership is reproducible for a committed version and this configuration,
    independent of API order, framework, source, epoch, ranks, and workers.
    Adding episodes or constraints in another version can merge groups and change
    assignments. Save the version and split configuration with model artifacts.
    """

    train: float = 0.8
    validate: float = 0.1
    eval: float = 0.1
    seed: int = 0
    groups: Mapping[str, str] | None = None

    def __post_init__(self) -> None:
        groups = self.groups
        proportions = (self.train, self.validate, self.eval)
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
        if groups is not None and (
            not isinstance(groups, Mapping)
            or any(
                not isinstance(key, str)
                or not key
                or not isinstance(value, str)
                or not value
                for key, value in groups.items()
            )
        ):
            raise ValueError(
                "Split groups must map nonempty episode IDs to nonempty group IDs"
            )
        object.__setattr__(
            self, "groups", None if groups is None else MappingProxyType(dict(groups))
        )

    def _select(
        self, recordings: Mapping[str, Sequence[str]], name: SplitName
    ) -> set[str]:
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
        assignments: dict[str, SplitName] = {}
        for episode_id in recordings:
            group = root(episode_id)
            if group not in assignments:
                payload = json.dumps(
                    ["foxglove-dataset-split-v1", self.seed, group],
                    separators=(",", ":"),
                ).encode()
                value = int.from_bytes(hashlib.sha256(payload).digest(), "big")
                scale = 1 << 256
                assignments[group] = (
                    "train"
                    if value < self.train * scale
                    else (
                        "validate"
                        if value < (self.train + self.validate) * scale
                        else "eval"
                    )
                )
        return {
            episode_id
            for episode_id in recordings
            if assignments[root(episode_id)] == name
        }
