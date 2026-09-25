from collections.abc import Iterator
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader

from .test_datasets import Client

torch = pytest.importorskip("torch")


def first_sample(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    yield {"id": next(episode.iter_messages())}


def test_spawned_workers_partition_ranks_without_duplicate_episodes() -> None:
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    ranks = []
    for rank in range(2):
        dataset = read_dataset(
            "dataset",
            version=7,
            topics=["/camera"],
            read_episode=first_sample,
            client_factory=Client,
            rank=rank,
            world_size=2,
        )
        loader = DataLoader(
            dataset, batch_size=None, num_workers=2, multiprocessing_context="spawn"
        )
        ranks.append([sample["id"] for sample in loader])
    assert sorted(ranks[0]) == ["a", "c"]
    assert sorted(ranks[1]) == ["b", "d"]


def test_repeated_epochs_create_fresh_streams() -> None:
    from foxglove.datasets.torch import read_dataset

    client = Client()
    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=first_sample,
        client_factory=lambda: client,
    )
    expected = [{"id": name} for name in ("a", "b", "c", "d")]
    assert list(dataset) == expected
    assert list(dataset) == expected
    assert client.closed == ["a", "b", "c", "d"] * 2


@pytest.mark.parametrize("rank, size", [(0, None), (None, 2), (-1, 2), (2, 2), (0, 0)])
def test_invalid_distributed_config(rank: Any, size: Any) -> None:
    from foxglove.datasets.torch import read_dataset

    with pytest.raises(ValueError):
        read_dataset(
            "dataset",
            version=7,
            topics=["/camera"],
            read_episode=first_sample,
            client_factory=Client,
            rank=rank,
            world_size=size,
        )


def test_rank_is_captured_before_worker_iteration(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from foxglove.datasets.torch import read_dataset
    from torch import distributed

    monkeypatch.setattr(distributed, "is_initialized", lambda: True)
    monkeypatch.setattr(distributed, "get_rank", lambda: 1)
    monkeypatch.setattr(distributed, "get_world_size", lambda: 2)
    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=first_sample,
        client_factory=Client,
    )
    monkeypatch.setattr(distributed, "is_initialized", lambda: False)
    assert list(dataset) == [{"id": "b"}, {"id": "d"}]
