import os
from collections.abc import Generator, Iterator
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


def test_incremental_blocks_and_sample_row_limit() -> None:
    from foxglove.datasets.ray import _Datasource
    from ray.data.block import BlockAccessor

    source = _Datasource(_plan("dataset", 7, ["/camera"], Client), many_samples)
    assert source.get_name() == "Foxglove"
    tasks = source.get_read_tasks(2, per_task_row_limit=300)
    assert len(tasks) == 2
    for task in tasks:
        blocks = list(task())
        assert [BlockAccessor.for_block(block).num_rows() for block in blocks] == [
            256,
            44,
        ]
        assert task.metadata.num_rows is None


def test_task_serialization_and_numpy_tensor_samples() -> None:
    import numpy as np
    from foxglove.datasets.ray import _Datasource
    from ray import cloudpickle
    from ray.data.block import BlockAccessor

    def image(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        yield {"id": episode.id, "image": np.ones((3, 4, 4), dtype=np.float32)}

    source = _Datasource(_plan("dataset", 7, ["/camera"], Client), image)
    tasks = cloudpickle.loads(cloudpickle.dumps(source.get_read_tasks(2)))
    rows = [
        row
        for task in tasks
        for block in task()
        for row in BlockAccessor.for_block(block).iter_rows(public_row_format=True)
    ]
    assert sorted(row["id"] for row in rows) == ["a", "b", "c", "d"]
    for row in rows:
        np.testing.assert_array_equal(row["image"], np.ones((3, 4, 4)))


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
    blocks = iter(source.get_read_tasks(1)[0]())
    assert isinstance(blocks, Generator)
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
    assert len(source.get_read_tasks(2, data_context=context)) == 2


def test_byte_target_flushes_before_row_limit() -> None:
    import numpy as np
    from foxglove.datasets.ray import _read_blocks
    from ray.data.block import BlockAccessor

    def large(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        for _ in range(10):
            yield {"image": np.ones((1024, 1024), dtype=np.uint8)}

    plan = _plan("dataset", 7, ["/camera"], Client)
    blocks = _read_blocks(plan, large, 10, 1024)
    first = next(blocks)
    assert BlockAccessor.for_block(first).num_rows() < 10
    blocks.close()


def test_native_ray_execution() -> None:
    from foxglove.datasets.ray import read_dataset

    dataset = read_dataset(
        "dataset",
        version=7,
        topics=["/camera"],
        read_episode=many_samples,
        client_factory=Client,
        concurrency=2,
    )
    assert dataset.count() == 4000


@pytest.mark.parametrize("sample", [(1, 2), 1, None])
def test_non_dictionary_rows_fail_with_episode_context_and_close_streams(
    sample: Any,
) -> None:
    from foxglove.datasets.ray import _Datasource

    client = Client()
    closed = []

    def invalid(episode: EpisodeReader) -> Iterator[Any]:
        messages = episode.iter_messages()
        next(messages)
        try:
            yield sample
        finally:
            closed.append(episode.id)

    source = _Datasource(_plan("dataset", 7, ["/camera"], lambda: client), invalid)
    with pytest.raises(
        RuntimeError, match="episode a.*dataset dataset version 7"
    ) as error:
        list(source.get_read_tasks(1)[0]())
    assert isinstance(error.value.__cause__, TypeError)
    assert str(error.value.__cause__) == (
        f"Ray read_episode must yield dictionaries, got {type(sample).__name__}"
    )
    assert closed == ["a"]
    assert client.closed == ["a"]
