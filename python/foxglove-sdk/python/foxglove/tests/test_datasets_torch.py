from collections.abc import Iterator
from dataclasses import dataclass
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader

from .test_datasets import Client

torch = pytest.importorskip("torch")


def first_sample(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    yield {"id": next(episode.iter_messages())[3]}


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


@pytest.mark.parametrize("with_target", [False, True])
def test_dataloader_batches_tensors_and_input_target_tuples(with_target: bool) -> None:
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    def sample(episode: EpisodeReader) -> Iterator[Any]:
        index = ord(episode.id) - ord("a")
        inputs = torch.tensor([index, index + 1])
        yield (inputs, index) if with_target else inputs

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=sample,
        client_factory=Client,
    )
    batches = list(DataLoader(dataset, batch_size=2))
    assert len(batches) == 2
    for index, batch in enumerate(batches):
        inputs = batch[0] if with_target else batch
        start = index * 2
        assert inputs.tolist() == [[start, start + 1], [start + 1, start + 2]]
        if with_target:
            assert batch[1].tolist() == [start, start + 1]


@dataclass
class Sample:
    episode_id: str


def custom_samples(episode: EpisodeReader) -> Iterator[Sample]:
    yield Sample(episode.id)


def collate_samples(samples: list[Sample]) -> list[str]:
    return [sample.episode_id for sample in samples]


def test_custom_sample_types_with_worker_collation() -> None:
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=custom_samples,
        client_factory=Client,
    )
    loader = DataLoader(
        dataset,
        batch_size=2,
        collate_fn=collate_samples,
        num_workers=1,
        multiprocessing_context="spawn",
    )
    assert list(loader) == [["a", "b"], ["c", "d"]]
