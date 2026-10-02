import os
from collections.abc import Iterator
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan

from .test_datasets import Client

# Use the current test environment, including the locally installed editable SDK,
# instead of having Ray recreate it with uv for each worker.
os.environ.setdefault("RAY_ENABLE_UV_RUN_RUNTIME_ENV", "0")
ray = pytest.importorskip("ray")


@pytest.fixture(scope="module", autouse=True)
def ray_runtime() -> Iterator[None]:
    ray.init(num_cpus=2, include_dashboard=False)
    try:
        yield
    finally:
        ray.shutdown()


def many_samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    for index in range(1000):
        yield {"id": episode.id, "index": index}


def test_sample_row_limit() -> None:
    from foxglove.datasets.ray import _Datasource
    from ray.data.block import BlockAccessor

    source = _Datasource(_plan("dataset", 7, ["/camera"], Client), many_samples)
    assert source.get_name() == "Foxglove"
    tasks = source.get_read_tasks(2, per_task_row_limit=300)
    assert len(tasks) == 2
    for task in tasks:
        blocks = list(task())
        assert sum(BlockAccessor.for_block(block).num_rows() for block in blocks) == 300


def test_read_task_cancellation_closes_message_stream() -> None:
    from foxglove.datasets.ray import _Datasource

    client = Client()
    closed = []

    def expand(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        messages = episode.iter_messages()
        name = next(messages)[3]
        try:
            for index in range(1000):
                yield {"id": name, "index": index}
        finally:
            closed.append(episode.id)

    source = _Datasource(_plan("dataset", 7, ["/camera"], lambda: client), expand)
    from ray.data.context import DataContext

    context = DataContext.get_current().copy()
    context.target_max_block_size = 1024
    blocks = iter(source.get_read_tasks(1, data_context=context)[0]())
    next(blocks)
    blocks.close()
    assert client.closed == ["a"]
    assert closed == ["a"]


def test_unlimited_target_max_block_size() -> None:
    from foxglove.datasets.ray import _Datasource
    from ray.data.context import DataContext

    context = DataContext.get_current().copy()
    context.target_max_block_size = None
    source = _Datasource(_plan("dataset", 7, ["/camera"], Client), many_samples)
    from ray.data.block import BlockAccessor

    tasks = source.get_read_tasks(2, data_context=context)
    assert len(tasks) == 2
    for task in tasks:
        assert [BlockAccessor.for_block(block).num_rows() for block in task()] == [2000]


def test_non_dictionary_rows_fail_with_episode_context_and_close_streams() -> None:
    from foxglove.datasets.ray import _Datasource

    client = Client()
    closed = []

    def invalid(episode: EpisodeReader) -> Iterator[Any]:
        messages = episode.iter_messages()
        next(messages)
        try:
            yield (1, 2)
        finally:
            closed.append(episode.id)

    source = _Datasource(_plan("dataset", 7, ["/camera"], lambda: client), invalid)
    with pytest.raises(
        RuntimeError, match="episode a.*dataset dataset version 7"
    ) as error:
        list(source.get_read_tasks(1)[0]())
    assert isinstance(error.value.__cause__, TypeError)
    assert str(error.value.__cause__) == (
        "Ray read_episode must yield mappings, got tuple"
    )
    assert closed == ["a"]
    assert client.closed == ["a"]


def test_native_ray_execution_with_large_callback_state() -> None:
    from foxglove.datasets.ray import _Datasource, read_dataset
    from ray import cloudpickle

    lookup = b"x" * (2 * 1024 * 1024)

    def sample(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        yield {"id": episode.id, "size": len(lookup)}

    source = _Datasource(_plan("dataset", 7, ["/camera"], Client), sample)
    assert len(cloudpickle.dumps(source.get_read_tasks(1)[0])) > 1024 * 1024
    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=sample,
        client_factory=Client,
    )
    assert sorted(dataset.take_all(), key=lambda row: row["id"]) == [
        {"id": episode_id, "size": len(lookup)} for episode_id in ("a", "b", "c", "d")
    ]


def test_native_ray_blocks_follow_context_target(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import numpy as np
    from foxglove.datasets.ray import _Datasource
    from ray.data.context import DataContext

    def sample(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        for index in range(1000):
            yield {
                "id": episode.id,
                "index": index,
                "values": np.full(1024, index % 256, dtype=np.uint8),
            }

    row_counts = []
    for target in (16 * 1024, 128 * 1024):
        monkeypatch.setattr(DataContext.get_current(), "target_max_block_size", target)
        source = _Datasource(_plan("dataset", 7, ["/camera"], Client), sample)
        # One read task isolates block shaping from Ray's automatic task splitting.
        dataset = ray.data.read_datasource(source, override_num_blocks=1)
        batches = list(dataset.iter_batches(batch_size=None, batch_format="numpy"))
        for batch in batches:
            expected = np.broadcast_to(
                (batch["index"] % 256).astype(np.uint8)[:, None], batch["values"].shape
            )
            np.testing.assert_array_equal(batch["values"], expected)
        assert sorted(
            (episode_id, int(index))
            for batch in batches
            for episode_id, index in zip(batch["id"], batch["index"])
        ) == [
            (episode_id, index)
            for episode_id in ("a", "b", "c", "d")
            for index in range(1000)
        ]
        counts = [len(batch["index"]) for batch in batches]
        assert sum(counts) == 4000
        assert len(counts) > 4
        row_counts.append(counts)
    assert max(row_counts[0]) < max(row_counts[1])
    assert len(row_counts[0]) > len(row_counts[1])


@pytest.mark.parametrize("direct", [False, True])
def test_video_callback_in_ray_workers(direct: bool) -> None:
    pytest.importorskip("av")
    import numpy as np
    from foxglove.datasets.ray import read_dataset
    from foxglove.datasets.video import decode_h264

    from .test_datasets_video import VideoClient, decode, messages, video_samples

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=decode_h264 if direct else video_samples,
        client_factory=VideoClient,
        concurrency=2,
    )
    rows = list(dataset.iter_rows())
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
        np.testing.assert_array_equal(actual["image"], frame["image"])


def test_ray_accepts_read_only_sample_mappings() -> None:
    from types import MappingProxyType

    from foxglove.datasets.ray import _read_rows
    from foxglove.datasets.reader import EpisodeReader, _plan

    client = Client()
    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    reader = EpisodeReader(plan.episodes[0], plan.topics, client)
    rows = list(
        _read_rows(lambda episode: [MappingProxyType({"id": episode.id})], reader)
    )
    assert rows == [{"id": "a"}]
    assert isinstance(rows[0], dict)
