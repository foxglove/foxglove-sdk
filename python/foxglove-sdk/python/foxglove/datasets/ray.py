"""Load committed datasets with Ray Data; requires ``foxglove-sdk[ray]``.

These APIs are experimental and unstable and may change in backward-incompatible ways.
"""

from __future__ import annotations

from collections.abc import Generator, Sequence
from contextlib import closing
from dataclasses import replace
from functools import partial
from itertools import islice
from typing import TYPE_CHECKING, Any

import pyarrow as pa
import ray.data
from ray.data.block import Block, BlockAccessor, BlockMetadata
from ray.data.context import DataContext
from ray.data.datasource import Datasource, ReadTask

from .reader import ClientFactory, EpisodeReader, ReadEpisode, _Plan, _plan

if TYPE_CHECKING:
    from ray.data._internal.block_builder import BlockBuilder


def _new_builder() -> BlockBuilder:
    return BlockAccessor.for_block(pa.table({})).builder()


def _read_rows(
    read_episode: ReadEpisode[dict[str, Any]], episode: EpisodeReader
) -> Generator[dict[str, Any], None, None]:
    samples = iter(read_episode(episode))
    try:
        for sample in samples:
            if not isinstance(sample, dict):
                raise TypeError(
                    "Ray read_episode must yield dictionaries, "
                    f"got {type(sample).__name__}"
                )
            yield sample
    finally:
        close = getattr(samples, "close", None)
        if close is not None:
            close()


def _read_blocks(
    plan: _Plan,
    read_episode: ReadEpisode[dict[str, Any]],
    row_limit: int | None,
    block_bytes: int | None,
) -> Generator[Block, None, None]:
    builder = _new_builder()
    with closing(
        plan.read(plan.episodes, partial(_read_rows, read_episode))
    ) as samples:
        for sample in islice(samples, row_limit):
            builder.add(sample)
            if (
                block_bytes is not None
                and builder.get_estimated_memory_usage() >= block_bytes
            ):
                yield builder.build()
                builder = _new_builder()
        if builder.num_rows():
            yield builder.build()


def _make_read_task(
    plan: _Plan,
    read_episode: ReadEpisode[dict[str, Any]],
    row_limit: int | None,
    block_bytes: int | None,
) -> ReadTask:
    # Ray uses the callable's name when warning about large serialized tasks.
    def read_blocks() -> Generator[Block, None, None]:
        return _read_blocks(plan, read_episode, row_limit, block_bytes)

    return ReadTask(
        read_blocks,
        BlockMetadata(
            num_rows=None, size_bytes=None, input_files=None, exec_stats=None
        ),
    )


class _Datasource(Datasource):
    def __init__(self, plan: _Plan, read_episode: ReadEpisode[dict[str, Any]]) -> None:
        self._plan = plan
        self._read_episode = read_episode

    def get_name(self) -> str:
        return "Foxglove"

    def estimate_inmemory_data_size(self) -> None:
        return None

    def get_read_tasks(
        self,
        parallelism: int,
        per_task_row_limit: int | None = None,
        data_context: DataContext | None = None,
    ) -> list[ReadTask]:
        if not self._plan.episodes:
            return []
        context = data_context or DataContext.get_current()
        count = max(1, min(parallelism, len(self._plan.episodes)))
        tasks = []
        for index in range(count):
            # Serialize only this task's episodes, not the whole plan.
            worker_plan = replace(
                self._plan, episodes=self._plan.episodes[index::count]
            )
            tasks.append(
                _make_read_task(
                    worker_plan,
                    self._read_episode,
                    per_task_row_limit,
                    context.target_max_block_size,
                )
            )
        return tasks


def read_dataset(
    dataset_id: str,
    *,
    version: int,
    topics: Sequence[str],
    read_episode: ReadEpisode[dict[str, Any]],
    client_factory: ClientFactory,
    concurrency: int | None = None,
) -> ray.data.Dataset:
    """Return a native Ray dataset of callback-produced sample dictionaries.

    .. warning::

        This API is experimental and unstable. It may change in backward-incompatible
        ways as we continue development and incorporate user feedback.

    Only episode metadata is fetched during planning. Workers stream selected
    topics and execute ``read_episode``. Factories and callbacks must be serializable
    and their dependencies available on every worker. Use consistent column types
    containing scalars, NumPy arrays, or other Arrow-compatible values.

    ``concurrency`` caps concurrent read tasks. Block construction follows Ray's
    ``DataContext.target_max_block_size``. Ray can combine blocks and buffer multiple
    episodes before delivering output. This target is not a memory ceiling: large
    samples, prefetching, and concurrent tasks can exceed it. Setting it to ``None``
    allows an entire task to be buffered. Read task retries rerun
    callbacks, so callbacks should not perform external side effects.
    """
    return ray.data.read_datasource(
        _Datasource(_plan(dataset_id, version, topics, client_factory), read_episode),
        concurrency=concurrency,
    )
