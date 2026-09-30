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


@pytest.mark.parametrize("rank, size", [(0, None), (2, 2), (0, 0)])
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


def test_dataloader_batches_input_target_tuples() -> None:
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    def sample(episode: EpisodeReader) -> Iterator[Any]:
        index = ord(episode.id) - ord("a")
        inputs = torch.tensor([index, index + 1])
        yield (inputs, index)

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
        inputs = batch[0]
        start = index * 2
        assert inputs.tolist() == [[start, start + 1], [start + 1, start + 2]]
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


def test_video_callback_in_spawned_workers_and_tensor_conversion() -> None:
    pytest.importorskip("av")
    from foxglove.datasets.torch import read_dataset, to_image_tensor
    from torch.utils.data import DataLoader

    from .test_datasets_video import VideoClient, decode, messages, video_samples

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=video_samples,
        client_factory=VideoClient,
    )
    loader = DataLoader(
        dataset, batch_size=None, num_workers=2, multiprocessing_context="spawn"
    )
    rows = list(loader)
    expected = decode(messages())
    assert len(rows) == 32
    for episode_id in ("a", "b", "c", "d"):
        selected = [row for row in rows if row["episode_id"] == episode_id]
        assert len(selected) == 8
        for actual, frame in zip(selected, expected):
            assert actual["timestamp_ns"] == frame.timestamp_ns
            assert actual["log_time_ns"] == frame.log_time_ns
            assert torch.equal(actual["image"], torch.from_numpy(frame.image))
    tensor = to_image_tensor(expected[0])
    assert tensor.shape == (3, 32, 48)
    assert tensor.dtype == torch.uint8 and tensor.is_contiguous()
    assert torch.equal(tensor, torch.from_numpy(expected[0].image).permute(2, 0, 1))
