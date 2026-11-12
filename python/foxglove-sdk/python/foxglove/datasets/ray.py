"""Load committed datasets with Ray Data; requires ``foxglove-sdk[ray]``."""

from __future__ import annotations

from collections.abc import Generator, Mapping, Sequence
from contextlib import closing
from dataclasses import replace
from functools import partial
from itertools import islice
from typing import TYPE_CHECKING, Any, Literal

import pyarrow as pa
import ray.data
from ray.data.block import Block, BlockAccessor, BlockMetadata
from ray.data.context import DataContext
from ray.data.datasource import Datasource, ReadTask

from .reader import ClientFactory, EpisodeReader, ReadEpisode, _Plan, _plan
from .splits import DatasetSplit, SplitName
from .storage import ObjectStoreFactory

if TYPE_CHECKING:
    from ray.data._internal.block_builder import BlockBuilder


def _new_builder() -> BlockBuilder:
    return BlockAccessor.for_block(pa.table({})).builder()


def _read_rows(
    read_episode: ReadEpisode[Mapping[str, Any]], episode: EpisodeReader
) -> Generator[dict[str, Any], None, None]:
    samples = iter(read_episode(episode))
    try:
        for sample in samples:
            if not isinstance(sample, Mapping):
                raise TypeError(
                    "Ray read_episode must yield mappings, "
                    f"got {type(sample).__name__}"
                )
            yield dict(sample)
    finally:
        close = getattr(samples, "close", None)
        if close is not None:
            close()


def _read_blocks(
    plan: _Plan,
    read_episode: ReadEpisode[Mapping[str, Any]],
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
    read_episode: ReadEpisode[Mapping[str, Any]],
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
    def __init__(
        self, plan: _Plan, read_episode: ReadEpisode[Mapping[str, Any]]
    ) -> None:
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
    read_episode: ReadEpisode[Mapping[str, Any]],
    source: Literal["foxglove", "object_storage"] = "foxglove",
    split_config: DatasetSplit | None = None,
    split: SplitName | None = None,
    include_properties: bool = False,
    client_factory: ClientFactory | None = None,
    object_store_factory: ObjectStoreFactory | None = None,
    concurrency: int | None = None,
) -> ray.data.Dataset:
    """Return a native Ray dataset of callback-produced sample mappings.

    Planning fetches only the episode list (ids, time windows, and, for object storage,
    recording locations). Workers stream selected topics and execute ``read_episode``.
    Factories and callbacks must be serializable and their dependencies available on
    every worker.

    :param dataset_id: Required. ID of the dataset.
    :param version: Required. Number of a committed dataset version.
    :param topics: Required. Topics to stream. Must be nonempty.
    :param read_episode: Required. Callback that receives each
        :class:`~foxglove.datasets.EpisodeReader` and yields sample mappings.
        Mappings (including TypedDict samples) are materialized as dictionaries for
        Ray. Use consistent column types containing scalars, NumPy arrays, or other
        Arrow-compatible values. Read task retries rerun callbacks, so callbacks
        should not perform external side effects.
    :param source: Optional. ``"foxglove"`` downloads messages through Foxglove.
        ``"object_storage"`` reads indexed MCAPs directly from S3, GCS, or Azure.
        The SDK selects the filesystem from recording locations and uses each
        worker's cloud credentials. Each MCAP file must have a summary. A file
        needs chunk indexes unless its summary reports zero messages, and every
        chunk that contains messages needs message indexes. Missing locations,
        unsupported schemes, and access failures raise errors.
    :param split_config: Optional. Together with ``split``, selects a
        recording-safe train, validation, or test pool before task partitioning.
        See :class:`foxglove.datasets.DatasetSplit`.
    :param split: Optional. The pool to read: ``"train"``, ``"validation"``, or
        ``"test"``. Requires ``split_config``.
    :param include_properties: Optional. ``True`` fetches each episode's
        properties from its pinned version for ``EpisodeReader.properties``. They
        are not fetched by default.
    :param client_factory: Optional. Callable that returns the API client,
        overriding API authentication or endpoints. The default client uses
        ``FOXGLOVE_API_TOKEN``. For object storage the API client factory is only
        used during planning and is not serialized.
    :param object_store_factory: Optional. Callable that returns an
        :class:`~foxglove.datasets.ObjectStore`, overriding built-in storage.
        Requires ``source="object_storage"``. Factories used by workers must be
        serializable. No storage clients or open files are serialized in the read
        tasks.
    :param concurrency: Optional. Caps concurrent read tasks. By default, Ray
        decides.
    :returns: A Ray dataset whose rows are the mappings that ``read_episode``
        yields. Block construction follows Ray's
        ``DataContext.target_max_block_size``. Ray can combine blocks and buffer
        multiple episodes before delivering output. Setting
        ``target_max_block_size`` to ``None`` allows an entire task to be buffered.
    :raises ValueError: If an argument is invalid, ``FOXGLOVE_API_TOKEN`` is not
        set and no ``client_factory`` is given, the version is not committed or
        contains missing recordings, ``split_config.groups`` does not cover exactly
        the version's episodes, an episode has no recording IDs when
        ``split_config`` is given, or, for object storage, a recording has no
        supported location.
    :raises RuntimeError: When the dataset is executed, if ``read_episode`` or a
        message stream raises, or ``read_episode`` yields a value that is not a
        mapping. The message names the episode, dataset, and version. Ray reports
        it as a ``RayTaskError`` that is also a ``RuntimeError`` and whose message
        contains the original traceback.

    Example::

        from foxglove.datasets.ray import read_dataset

        def read_episode(episode):
            for schema, channel, message, payload in episode.iter_messages():
                yield {"log_time": message.log_time}

        ds = read_dataset(
            "dset_123", version=2, topics=["/camera"], read_episode=read_episode
        )
    """
    return ray.data.read_datasource(
        _Datasource(
            _plan(
                dataset_id,
                version,
                topics,
                client_factory,
                object_store_factory,
                source=source,
                split_config=split_config,
                split=split,
                include_properties=include_properties,
            ),
            read_episode,
        ),
        concurrency=concurrency,
    )
