"""Load committed datasets with Ray Data; requires ``foxglove-sdk[ray]``."""

from __future__ import annotations

import inspect
from collections.abc import Generator, Sequence
from contextlib import closing
from dataclasses import replace
from functools import partial
from itertools import islice
from typing import Any

import pyarrow as pa
import ray.data
from ray.data.block import Block, BlockAccessor, BlockMetadata
from ray.data.context import DataContext
from ray.data.datasource import Datasource, ReadTask

from .reader import ClientFactory, ReadEpisode, _Plan, _plan

_BLOCK_BYTES = 8 * 1024 * 1024
_BLOCK_ROWS = 256


def _read_blocks(
    plan: _Plan, read_episode: ReadEpisode, row_limit: int | None, block_bytes: int
) -> Generator[Block, None, None]:
    builder = BlockAccessor.for_block(pa.table({})).builder()
    with closing(plan.read(plan.episodes, read_episode)) as samples:
        for sample in islice(samples, row_limit):
            builder.add(sample)
            if (
                builder.num_rows() >= _BLOCK_ROWS
                or builder.get_estimated_memory_usage() >= block_bytes
            ):
                yield builder.build()
                builder = BlockAccessor.for_block(pa.table({})).builder()
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
        block_bytes = min(_BLOCK_BYTES, context.target_max_block_size)
        count = max(1, min(parallelism, len(self._plan.episodes)))
        tasks = []
        for index in range(count):
            worker_plan = replace(
                self._plan, episodes=self._plan.episodes[index::count]
            )
            # Older supported Ray releases also require a schema argument.
            metadata_args: dict[str, Any] = {
                "num_rows": None,
                "size_bytes": None,
                "input_files": None,
                "exec_stats": None,
            }
            if "schema" in inspect.signature(BlockMetadata).parameters:
                metadata_args["schema"] = None
            tasks.append(
                ReadTask(
                    partial(
                        _read_blocks,
                        worker_plan,
                        self._read_episode,
                        per_task_row_limit,
                        block_bytes,
                    ),
                    BlockMetadata(**metadata_args),
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
    if concurrency is not None and (
        isinstance(concurrency, bool)
        or not isinstance(concurrency, int)
        or concurrency < 1
    ):
        raise ValueError("concurrency must be a positive integer")
    return ray.data.read_datasource(
        _Datasource(_plan(dataset_id, version, topics, client_factory), read_episode),
        concurrency=concurrency,
    )
