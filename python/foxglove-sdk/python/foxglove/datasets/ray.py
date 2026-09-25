"""Load committed datasets with Ray Data; requires ``foxglove-sdk[ray]``."""

from __future__ import annotations

from collections.abc import Generator, Sequence
from contextlib import closing
from dataclasses import replace
from functools import partial
from itertools import islice
from typing import TYPE_CHECKING

import pyarrow as pa
import ray.data
from ray.data.block import Block, BlockAccessor, BlockMetadata
from ray.data.context import DataContext
from ray.data.datasource import Datasource, ReadTask

from .reader import ClientFactory, ReadEpisode, _Plan, _plan

if TYPE_CHECKING:
    from ray.data._internal.block_builder import BlockBuilder

_BLOCK_BYTES = 8 * 1024 * 1024
_BLOCK_ROWS = 256


def _new_builder() -> BlockBuilder:
    return BlockAccessor.for_block(pa.table({})).builder()


def _read_blocks(
    plan: _Plan, read_episode: ReadEpisode, row_limit: int | None, block_bytes: int
) -> Generator[Block, None, None]:
    builder = _new_builder()
    with closing(plan.read(plan.episodes, read_episode)) as samples:
        for sample in islice(samples, row_limit):
            builder.add(sample)
            if (
                builder.num_rows() >= _BLOCK_ROWS
                or builder.get_estimated_memory_usage() >= block_bytes
            ):
                yield builder.build()
                builder = _new_builder()
        if builder.num_rows():
            yield builder.build()


class _Datasource(Datasource):
    def __init__(self, plan: _Plan, read_episode: ReadEpisode) -> None:
        self._plan = plan
        self._read_episode = read_episode

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
        limit = context.target_max_block_size
        block_bytes = _BLOCK_BYTES if limit is None else min(_BLOCK_BYTES, limit)
        count = max(1, min(parallelism, len(self._plan.episodes)))
        tasks = []
        for index in range(count):
            # Serialize only this task's episodes, not the whole plan.
            worker_plan = replace(
                self._plan, episodes=self._plan.episodes[index::count]
            )
            tasks.append(
                ReadTask(
                    partial(
                        _read_blocks,
                        worker_plan,
                        self._read_episode,
                        per_task_row_limit,
                        block_bytes,
                    ),
                    BlockMetadata(
                        num_rows=None,
                        size_bytes=None,
                        input_files=None,
                        exec_stats=None,
                    ),
                )
            )
        return tasks


def read_dataset(
    dataset_id: str,
    *,
    version: int,
    topics: Sequence[str],
    read_episode: ReadEpisode,
    client_factory: ClientFactory,
    concurrency: int | None = None,
) -> ray.data.Dataset:
    """Return a native Ray dataset of callback-produced sample dictionaries.

    Only episode metadata is fetched during planning. Workers stream selected
    topics and execute ``read_episode``. Factories and callbacks must be serializable
    and their dependencies available on every worker. Use consistent column types
    containing scalars, NumPy arrays, or other Arrow-compatible values.

    ``concurrency`` caps concurrent read tasks. Output blocks target at most 256
    samples or 8 MiB of estimated data (a single sample can exceed that size).
    Ray can prefetch beyond the current consumer demand. Read task retries rerun
    callbacks, so callbacks should not perform external side effects.
    """
    return ray.data.read_datasource(
        _Datasource(_plan(dataset_id, version, topics, client_factory), read_episode),
        concurrency=concurrency,
    )
