from collections.abc import Iterator
from dataclasses import dataclass
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader

from .datasets_helpers import Client

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


@pytest.mark.parametrize("direct", [False, True])
def test_video_callback_in_spawned_workers_and_tensor_conversion(direct: bool) -> None:
    pytest.importorskip("av")
    from foxglove.datasets.torch import read_dataset, to_image_tensor
    from foxglove.datasets.video import decode_h264
    from torch.utils.data import DataLoader

    from .test_datasets_video import VideoClient, decode, messages, video_samples

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=decode_h264 if direct else video_samples,
        client_factory=VideoClient,
    )
    loader = DataLoader(
        dataset, batch_size=None, num_workers=2, multiprocessing_context="spawn"
    )
    rows = list(loader)
    expected = decode(messages())
    assert len(rows) == 32
    if not direct:
        assert sorted(row["episode_id"] for row in rows) == [
            episode_id for episode_id in ("a", "b", "c", "d") for _ in range(8)
        ]
    ordered = sorted(rows, key=lambda row: row["log_time_ns"])
    for actual, frame in zip(ordered, [frame for frame in expected for _ in range(4)]):
        assert actual["timestamp_ns"] == frame["timestamp_ns"]
        assert actual["log_time_ns"] == frame["log_time_ns"]
        assert torch.equal(actual["image"], torch.from_numpy(frame["image"]))
    frame = expected[0]
    tensor = to_image_tensor(frame)
    assert tensor.shape == (3, 32, 48)
    assert tensor.dtype == torch.uint8 and tensor.is_contiguous()
    assert torch.equal(tensor, torch.from_numpy(expected[0]["image"]).permute(2, 0, 1))


@pytest.mark.parametrize("source", ["numpy", "tensor", "loader"])
@pytest.mark.parametrize("batch_size", [None, 2])
@pytest.mark.parametrize("dtype", ["uint8", "float32"])
def test_image_tensor_accepts_arrays_tensors_and_loader_batches(
    source: str, batch_size: int | None, dtype: str
) -> None:
    import numpy as np
    from foxglove.datasets.torch import to_image_tensor
    from torch.utils.data import DataLoader

    images = np.arange(120).reshape(2, 4, 5, 3).astype(dtype)
    original = images[0] if batch_size is None else images
    if source == "loader":
        frame = next(
            iter(
                DataLoader(
                    [{"image": image} for image in images], batch_size=batch_size
                )
            )
        )
    else:
        frame = {
            "image": torch.from_numpy(original) if source == "tensor" else original
        }
    actual = to_image_tensor(frame)
    expected = torch.stack(
        [torch.from_numpy(original[..., channel]) for channel in range(3)],
        dim=0 if batch_size is None else 1,
    )
    assert actual.shape == expected.shape
    assert actual.dtype == expected.dtype
    assert actual.device == expected.device
    assert actual.is_contiguous()
    assert torch.equal(actual, expected)


@pytest.mark.parametrize("shape", [(4, 5), (4, 5, 1), (4, 5, 4)])
def test_image_tensor_rejects_invalid_layouts(shape: tuple[int, ...]) -> None:
    from foxglove.datasets.torch import to_image_tensor

    with pytest.raises(ValueError, match="channels-last RGB"):
        to_image_tensor({"image": torch.zeros(shape)})


@pytest.mark.parametrize("shape", [(4, 5, 3), (2, 4, 5, 3)])
def test_image_tensor_rejects_converting_channels_first_twice(
    shape: tuple[int, ...],
) -> None:
    from foxglove.datasets.torch import to_image_tensor

    converted = to_image_tensor({"image": torch.zeros(shape)})
    with pytest.raises(ValueError, match="channels-last RGB"):
        to_image_tensor({"image": converted})


def test_seeded_episode_order_and_epoch_replay() -> None:
    from foxglove.datasets.torch import TorchDataset, read_dataset

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=first_sample,
        client_factory=Client,
        shuffle=True,
        seed=17,
    )
    assert isinstance(dataset, TorchDataset)
    orders = []
    for epoch in (0, 1, 2, 0):
        dataset.set_epoch(epoch)
        rows = [row["id"] for row in dataset]
        assert sorted(rows) == ["a", "b", "c", "d"]
        orders.append(rows)
    assert orders[0] == orders[3]
    assert len({tuple(order) for order in orders[:3]}) == 3
    replica = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=first_sample,
        client_factory=Client,
        shuffle=True,
        seed=17,
    )
    assert [row["id"] for row in replica] == orders[0]


def test_epoch_change_preserves_active_iterator_and_unshuffled_order() -> None:
    from foxglove.datasets.torch import read_dataset

    for shuffle in (False, True):
        dataset = read_dataset(
            "dataset",
            version=7,
            topics=["/camera"],
            read_episode=first_sample,
            client_factory=Client,
            shuffle=shuffle,
            seed=17,
        )
        expected = list(dataset)
        active = iter(dataset)
        dataset.set_epoch(1)
        assert list(active) == expected
        if not shuffle:
            assert list(dataset) == expected == [{"id": name} for name in "abcd"]


def test_persistent_spawned_workers_receive_epoch_updates() -> None:
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=first_sample,
        client_factory=Client,
        shuffle=True,
        seed=17,
    )
    loader = DataLoader(
        dataset,
        batch_size=None,
        num_workers=2,
        multiprocessing_context="spawn",
        persistent_workers=True,
    )
    orders = []
    for epoch in (0, 1, 2, 0):
        dataset.set_epoch(epoch)
        expected = list(dataset)
        rows = list(loader)
        assert rows == expected
        assert sorted(row["id"] for row in rows) == list("abcd")
        orders.append(rows)
    assert orders[0] == orders[3]
    assert orders[0] != orders[1]


@pytest.mark.parametrize(
    "world_size, worker_counts", [(2, [1, 2]), (5, [0, 1, 2, 0, 1])]
)
def test_shuffled_rank_first_partitioning_with_unequal_workers(
    world_size: int,
    worker_counts: list[int],
) -> None:
    from foxglove.datasets.torch import read_dataset
    from torch.utils.data import DataLoader

    combined: list[str] = []
    for rank, num_workers in enumerate(worker_counts):
        dataset = read_dataset(
            "dataset",
            version=7,
            topics=["/camera"],
            read_episode=first_sample,
            client_factory=Client,
            shuffle=True,
            seed=17,
            rank=rank,
            world_size=world_size,
        )
        dataset.set_epoch(2)
        loader = DataLoader(
            dataset,
            batch_size=None,
            num_workers=num_workers,
            **({"multiprocessing_context": "spawn"} if num_workers else {}),
        )
        rows = list(loader)
        assert rows == list(dataset)
        if rank >= 4:
            assert rows == []
        combined.extend(row["id"] for row in rows)
    assert sorted(combined) == list("abcd")


@pytest.mark.parametrize("epoch", [-1, 2**63, 1.5, "1", True])
def test_invalid_epoch(epoch: Any) -> None:
    from foxglove.datasets.torch import read_dataset

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=first_sample,
        client_factory=Client,
    )
    with pytest.raises((TypeError, ValueError), match="epoch"):
        dataset.set_epoch(epoch)


@pytest.mark.parametrize("kwargs", [{"shuffle": 1}, {"seed": 1.5}, {"seed": True}])
def test_invalid_shuffle_configuration(kwargs: dict[str, Any]) -> None:
    from foxglove.datasets.torch import read_dataset

    with pytest.raises(TypeError):
        read_dataset(
            "dataset",
            version=7,
            topics=["/camera"],
            read_episode=first_sample,
            client_factory=Client,
            **kwargs,
        )


def test_unshuffled_dataset_does_not_allocate_shared_memory(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from foxglove.datasets.torch import read_dataset

    def fail_shared_memory(_self: Any) -> None:
        raise AssertionError("unshuffled datasets do not require shared memory")

    monkeypatch.setattr(torch.Tensor, "share_memory_", fail_shared_memory)
    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=first_sample,
        client_factory=Client,
    )
    dataset.set_epoch(1)
    assert list(dataset) == [{"id": name} for name in "abcd"]
