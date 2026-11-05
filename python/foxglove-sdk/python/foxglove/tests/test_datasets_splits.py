"""Recording-safe dataset split membership and adapter planning."""

from collections.abc import Iterator
from typing import Any

import pytest
from foxglove.datasets import DatasetSplit, EpisodeReader, SplitSummary, inspect_splits
from foxglove.datasets.reader import _Plan, _plan

from .datasets_helpers import Client, Page


class SplitPage(Page):
    def __init__(self, entries: list[dict[str, Any]]) -> None:
        self.entries = entries

    def auto_paging_iter(self) -> Iterator[dict[str, Any]]:
        yield from self.entries


class SplitClient(Client):
    def __init__(self, entries: Any = None) -> None:
        super().__init__()
        self.entries = list(Page().auto_paging_iter()) if entries is None else entries
        for entry in self.entries:
            entry["episode"].setdefault("recordings", [{"id": entry["episode"]["id"]}])
        self.includes: list[bool] = []

    def get_dataset_version_episodes(self, **kwargs: Any) -> Any:
        self.includes.append(kwargs["include_recordings"])
        return SplitPage(self.entries)


def plan(client: Any, split: Any = None, name: Any = None, **kwargs: Any) -> _Plan:
    return _plan(
        "dataset",
        7,
        ["/topic"],
        lambda: client,
        split_config=split,
        split=name,
        **kwargs,
    )


def pools(client: Any, split: Any) -> Any:
    return {
        name: {episode.id for episode in plan(client, split, name).episodes}
        for name in ("train", "validation", "test")
    }


def test_membership_covers_version_and_ignores_pagination_order() -> None:
    client = SplitClient()
    split = DatasetSplit(seed=17)
    first = pools(client, split)
    client.entries.reverse()
    assert pools(client, split) == first
    assert set.union(*first.values()) == {"a", "b", "c", "d"}
    assert sum(map(len, first.values())) == 4
    assert all(client.includes)
    assert all(ep.version == 2 for ep in plan(client, split, "train").episodes)


def test_recording_components_and_custom_groups_add_constraints() -> None:
    client = SplitClient()
    ids = {"a": ["r1"], "b": ["r1", "r2"], "c": ["r2"], "d": ["r3"]}
    for entry in client.entries:
        entry["episode"]["recordings"] = [
            {"id": rid} for rid in ids[entry["episode"]["id"]]
        ]
    for seed in range(20):
        result = pools(client, DatasetSplit(seed=seed))
        assert any({"a", "b", "c"} <= membership for membership in result.values())
        combined = pools(
            client,
            DatasetSplit(seed=seed, groups={"a": "x", "b": "y", "c": "z", "d": "z"}),
        )
        assert any(
            membership == {"a", "b", "c", "d"} for membership in combined.values()
        )


def test_seed_changes_membership_without_using_global_randomness() -> None:
    recordings = {str(index): (str(index),) for index in range(100)}
    assert DatasetSplit(seed=1)._select(recordings, "train") != DatasetSplit(
        seed=2
    )._select(recordings, "train")


def test_disabled_split_preserves_request_and_empty_pools_are_valid() -> None:
    client = SplitClient()
    assert len(plan(client).episodes) == 4
    assert client.includes == [False]
    result = pools(client, DatasetSplit(train=1, validation=0, test=0))
    assert result == {"train": {"a", "b", "c", "d"}, "validation": set(), "test": set()}
    assert pools(SplitClient([]), DatasetSplit()) == {
        "train": set(),
        "validation": set(),
        "test": set(),
    }


@pytest.mark.parametrize(
    "kwargs",
    [
        dict(train=-1),
        dict(train=float("nan")),
        dict(test=float("inf")),
        dict(train=0),
        dict(seed=True),
        dict(seed=1.5),
        dict(groups={"a": ""}),
    ],
)
def test_invalid_configuration(kwargs: Any) -> None:
    with pytest.raises(ValueError):
        DatasetSplit(**kwargs)


@pytest.mark.parametrize(
    "recordings", [None, [], [{"id": ""}], [{"id": 4}], [{}], ["rec"]]
)
def test_missing_recording_ids_fail_closed(recordings: Any) -> None:
    client = SplitClient()
    client.entries[0]["episode"]["recordings"] = recordings
    with pytest.raises(ValueError, match="recording IDs"):
        plan(client, DatasetSplit(), "train")


def test_group_mapping_is_snapshotted_and_requires_complete_version() -> None:
    mapping = {key: key for key in "abcd"}
    split = DatasetSplit(groups=mapping)
    before = pools(SplitClient(), split)
    mapping["a"] = "b"
    assert pools(SplitClient(), split) == before
    with pytest.raises(ValueError, match="cover exactly"):
        plan(SplitClient(), DatasetSplit(groups={"a": "x"}), "train")


@pytest.mark.parametrize(
    "split,name", [(None, "train"), (DatasetSplit(), None), (DatasetSplit(), "eval")]
)
def test_split_name_validation(split: Any, name: Any) -> None:
    with pytest.raises(ValueError):
        plan(SplitClient(), split, name)


def test_inspection_counts_connected_groups_without_reading_messages() -> None:
    client = SplitClient()
    recording_ids = {"a": ["r1"], "b": ["r1", "r2"], "c": ["r2"], "d": ["r3"]}
    for entry in client.entries:
        entry["episode"]["recordings"] = [
            {"id": recording_id}
            for recording_id in recording_ids[entry["episode"]["id"]]
        ]
    for seed in range(10):
        config = DatasetSplit(seed=seed)
        summary = inspect_splits(
            "dataset", version=7, split_config=config, client_factory=lambda: client
        )
        assert sum(counts.group_count for counts in summary.values()) == 2
        assert sum(counts.episode_count for counts in summary.values()) == 4
        for name, membership in pools(client, config).items():
            assert summary[name].episode_count == len(membership)
            expected_groups = int("a" in membership) + int("d" in membership)
            assert summary[name].group_count == expected_groups
    assert client.calls == []
    assert all(client.includes)
    config = DatasetSplit(
        train=1, validation=0, test=0, groups=dict.fromkeys("abcd", "site")
    )
    assert inspect_splits(
        "dataset", version=7, split_config=config, client_factory=lambda: client
    ) == {
        "train": SplitSummary(group_count=1, episode_count=4),
        "validation": SplitSummary(group_count=0, episode_count=0),
        "test": SplitSummary(group_count=0, episode_count=0),
    }


def test_inspection_preserves_empty_splits() -> None:
    summary = inspect_splits(
        "dataset",
        version=7,
        split_config=DatasetSplit(),
        client_factory=lambda: SplitClient([]),
    )
    assert summary == {
        "train": SplitSummary(group_count=0, episode_count=0),
        "validation": SplitSummary(group_count=0, episode_count=0),
        "test": SplitSummary(group_count=0, episode_count=0),
    }


@pytest.mark.parametrize(
    "invalid", ["uncommitted", "missing", "recording_ids", "groups"]
)
def test_inspection_rejects_invalid_metadata(invalid: str) -> None:
    client = SplitClient()
    config = DatasetSplit()
    if invalid in ("uncommitted", "missing"):
        client.get_dataset_version = lambda **kwargs: {  # type: ignore[method-assign]
            "committed_at": None if invalid == "uncommitted" else "2026-01-01",
            "has_missing_recordings": invalid == "missing",
        }
    elif invalid == "recording_ids":
        client.entries[0]["episode"]["recordings"] = []
    else:
        config = DatasetSplit(groups={"a": "site"})
    with pytest.raises(ValueError):
        inspect_splits(
            "dataset", version=7, split_config=config, client_factory=lambda: client
        )


def test_object_storage_uses_same_membership_without_requiring_locations_for_foxglove() -> (
    None
):
    client = SplitClient()
    expected = pools(client, DatasetSplit(seed=2))
    for entry in client.entries:
        for recording in entry["episode"]["recordings"]:
            recording.update(
                available=True,
                location={
                    "scheme": "s3",
                    "bucket": "bucket",
                    "path": f"{recording['id']}.mcap",
                },
            )
    for name, ids in expected.items():
        result = plan(
            client,
            DatasetSplit(seed=2),
            name,
            source="object_storage",
            object_store_factory=lambda: None,
        )
        assert {episode.id for episode in result.episodes} == ids


def test_adapters_select_same_pool_and_read_pinned_episode_versions(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pytest.importorskip("torch")
    ray = pytest.importorskip("ray")
    from foxglove.datasets.ray import read_dataset as read_ray
    from foxglove.datasets.torch import read_dataset as read_torch

    def first(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        yield {"id": next(episode.iter_messages())[3]}

    def consume(source: Any, **kwargs: Any) -> Any:
        return list(source._plan.read(source._plan.episodes, source._read_episode))

    monkeypatch.setattr(ray.data, "read_datasource", consume)
    for name in ("train", "validation", "test"):
        client = SplitClient()
        split = DatasetSplit(train=0.5, validation=0.25, test=0.25, seed=8)
        arguments: dict[str, Any] = dict(
            version=7,
            topics=["/topic"],
            read_episode=first,
            client_factory=lambda: client,
            split_config=split,
            split=name,
        )
        expected = {ep.id for ep in plan(client, split, name).episodes}
        ray_rows = read_ray("dataset", **arguments)
        assert {row["id"] for row in ray_rows} == expected
        torch_rows: list[dict[str, Any]] = []
        for rank in range(3):
            dataset = read_torch("dataset", rank=rank, world_size=3, **arguments)
            torch_rows.extend(dataset)
        assert {row["id"] for row in torch_rows} == expected
        assert len(torch_rows) == len(expected)
