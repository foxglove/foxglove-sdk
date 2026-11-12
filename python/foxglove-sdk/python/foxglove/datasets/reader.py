"""Shared planning and streaming, using the Foxglove Python API client."""

from __future__ import annotations

import os
from collections.abc import Callable, Generator, Iterable, Iterator, Mapping, Sequence
from contextlib import ExitStack
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, Literal, Protocol, TypeVar, cast

from .splits import DatasetSplit, SplitName, SplitSummary
from .storage import (
    ObjectLocation,
    ObjectStore,
    ObjectStoreFactory,
    _CloudObjectStore,
    _decoder_not_found_hint,
    _MessageIds,
    _validate_location,
)

if TYPE_CHECKING:
    from mcap.decoder import DecoderFactory
    from mcap.records import Channel, Message, Schema

_T = TypeVar("_T")

_EPISODE_PAGE_SIZE = 2000


class _Page(Protocol):
    def auto_paging_iter(self) -> Iterator[dict[str, Any]]: ...


class _Client(Protocol):
    def get_dataset_version(
        self, *, dataset_id: str, version_number: int
    ) -> Mapping[str, Any]: ...

    def get_dataset_version_episodes(
        self,
        *,
        dataset_id: str,
        version_number: int,
        limit: int,
        include_recordings: bool = False,
        include_properties: bool = False,
    ) -> _Page: ...

    def iter_messages(
        self,
        *,
        episode_id: str,
        episode_version: int,
        topics: list[str],
        decoder_factories: list[DecoderFactory] | None = None,
        start: datetime | None = None,
        end: datetime | None = None,
    ) -> Generator[tuple[Schema | None, Channel, Message, Any], None, None]: ...


ClientFactory = Callable[[], _Client]


def _make_client() -> _Client:
    from foxglove.client import Client

    token = os.environ.get("FOXGLOVE_API_TOKEN")
    if not token:
        raise ValueError("Set FOXGLOVE_API_TOKEN or provide client_factory")
    return cast(_Client, Client(token=token))


@dataclass(frozen=True)
class _Episode:
    id: str
    start_time: datetime
    end_time: datetime
    locations: tuple[ObjectLocation, ...] = ()
    version: int = field(kw_only=True)
    # None when properties were not requested during planning.
    properties: dict[str, Any] | None = field(default=None, kw_only=True)


class EpisodeReader:
    """An episode scoped to the topics selected by ``read_dataset``.

    Instances are supplied to the user's ``read_episode`` callback. The SDK closes
    an instance when that callback's iterator finishes or is closed. Message tuples
    are the client's ``(schema, channel, message, decoded_message)`` values. Create
    custom message decoders inside the callback and pass them to :meth:`iter_messages`.
    """

    def __init__(
        self,
        episode: _Episode,
        topics: tuple[str, ...],
        client: _Client | None,
        object_store: ObjectStore | None = None,
    ) -> None:
        self._episode = episode
        self._topics = topics
        self._client = client
        self._object_store = object_store
        self._message_ids = _MessageIds()
        self._streams: set[Generator[Any, None, None]] = set()
        self._closed = False

    @property
    def id(self) -> str:
        """Episode ID."""
        return self._episode.id

    @property
    def start_time(self) -> datetime:
        """Start of the episode's time window."""
        return self._episode.start_time

    @property
    def end_time(self) -> datetime:
        """End of the episode's time window."""
        return self._episode.end_time

    @property
    def properties(self) -> Mapping[str, Any]:
        """Read-only properties of the episode version pinned by the dataset version.

        Requires ``include_properties=True`` in ``read_dataset``. Empty if the episode
        has no properties.

        :raises RuntimeError: If properties were not requested.
        """
        if self._episode.properties is None:
            raise RuntimeError(
                "Episode properties were not requested; "
                "pass include_properties=True to read_dataset"
            )
        return MappingProxyType(self._episode.properties)

    def iter_messages(
        self,
        *,
        decoder_factories: Sequence[DecoderFactory] | None = None,
        topics: Sequence[str] | None = None,
        lookback: timedelta = timedelta(0),
    ) -> Generator[tuple[Schema | None, Channel, Message, Any], None, None]:
        """Stream the selected topics in log-time order without buffering the episode.

        Each invocation opens a new stream on first iteration. Repeated invocations
        redownload data. Active streams are closed when the callback ends.
        Message context and stream order are preserved for stateful processing.

        :param decoder_factories: Optional. MCAP decoder factories used for message
            deserialization. ``None`` uses the client's default decoders; an explicit
            list replaces them. Construct factories inside the callback so their
            state stays local to the worker and episode.
        :param topics: Optional. Nonempty subset of the dataset topics; ``None``
            streams all of them. Filtering happens on the server or through MCAP
            indexes for object storage; separate calls open separate downloads.
        :param lookback: Optional. Additional history to request before the episode
            start, for stateful media decoding. Must be nonnegative. The history is
            read from the recordings attached to the episode. Consumers must
            exclude lookback messages, which are logged before :attr:`start_time`,
            from their training samples.
        :returns: Tuples of schema (possibly ``None``), channel, raw MCAP message,
            and decoded message payload.
        :raises ValueError: On first iteration, if ``lookback`` is negative, or
            ``topics`` is empty or not a subset of the dataset topics.
        :raises RuntimeError: On first iteration, if the callback that received
            this reader has ended.
        :raises mcap.exceptions.DecoderNotFoundError: If no decoder factory handles
            the encoding of a message.

        Example::

            def read_episode(episode):
                for schema, channel, message, payload in episode.iter_messages():
                    yield {
                        "topic": channel.topic,
                        "log_time": message.log_time,
                    }
        """
        if self._closed:
            raise RuntimeError("Episode reader is closed")
        if lookback < timedelta(0):
            raise ValueError("lookback must be nonnegative")
        selected_topics = (
            self._topics if topics is None else tuple(dict.fromkeys(topics))
        )
        if isinstance(topics, str) or not selected_topics:
            raise ValueError("topics must be a nonempty sequence of topic names")
        if not set(selected_topics).issubset(self._topics):
            raise ValueError("topics must be a subset of the dataset topics")
        if self._object_store is not None:
            from .storage import _iter_messages

            yield from self._manage(
                _iter_messages(
                    self._object_store,
                    self._episode.locations,
                    selected_topics,
                    self.start_time - lookback,
                    self.end_time,
                    decoder_factories,
                    self._message_ids,
                )
            )
            return
        if self._client is None:
            raise RuntimeError("Episode reader has no message source")
        time_range = (
            {"start": self.start_time - lookback, "end": self.end_time}
            if lookback
            else {}
        )
        stream = self._client.iter_messages(
            episode_id=self.id,
            episode_version=self._episode.version,
            topics=list(selected_topics),
            decoder_factories=(
                None if decoder_factories is None else list(decoder_factories)
            ),
            **time_range,
        )
        yield from self._manage(stream)

    def _manage(self, stream: Generator[_T, None, None]) -> Generator[_T, None, None]:
        """Keep a processing iterator alive only for this episode callback."""
        if self._closed:
            stream.close()
            raise RuntimeError("Episode reader is closed")

        def managed() -> Generator[_T, None, None]:
            try:
                yield from stream
            finally:
                self._streams.discard(result)
                stream.close()

        result = managed()
        self._streams.add(result)
        return result

    def _close(self) -> None:
        self._closed = True
        # Closing a processing iterator can also close its managed input stream.
        # Snapshot the set and attempt every close even if one raises.
        with ExitStack() as cleanup:
            for stream in tuple(self._streams):
                cleanup.callback(stream.close)
            self._streams.clear()


ReadEpisode = Callable[[EpisodeReader], Iterable[_T]]


@dataclass(frozen=True)
class _Plan:
    dataset_id: str
    version: int
    topics: tuple[str, ...]
    episodes: tuple[_Episode, ...]
    client_factory: ClientFactory | None
    object_store_factory: ObjectStoreFactory | None = None

    def read(
        self, episodes: Sequence[_Episode], read_episode: ReadEpisode[_T]
    ) -> Generator[_T, None, None]:
        if not episodes:
            return
        object_store = (
            self.object_store_factory()
            if self.object_store_factory is not None
            else None
        )
        client = self.client_factory() if self.client_factory is not None else None
        for episode in episodes:
            reader = EpisodeReader(episode, self.topics, client, object_store)
            samples = None
            try:
                samples = iter(read_episode(reader))
                for sample in samples:
                    yield sample
            except Exception as error:
                message = (
                    f"Failed to read episode {episode.id} in dataset "
                    f"{self.dataset_id} version {self.version}"
                )
                hint = _decoder_not_found_hint(error)
                if hint is not None:
                    message = f"{message}: {hint}"
                raise RuntimeError(message) from error
            finally:
                try:
                    close = getattr(samples, "close", None)
                    if close is not None:
                        close()
                finally:
                    reader._close()


def _plan(
    dataset_id: str,
    version: int,
    topics: Sequence[str],
    client_factory: ClientFactory | None = None,
    object_store_factory: ObjectStoreFactory | None = None,
    *,
    source: Literal["foxglove", "object_storage"] = "foxglove",
    split_config: DatasetSplit | None = None,
    split: SplitName | None = None,
    include_properties: bool = False,
) -> _Plan:
    if (split_config is None) != (split is None):
        raise ValueError("split_config and split must be provided together")
    if split_config is not None and not isinstance(split_config, DatasetSplit):
        raise ValueError("split_config must be a DatasetSplit")
    if split is not None and split not in ("train", "validation", "test"):
        raise ValueError("split must be train, validation, or test")
    if source not in ("foxglove", "object_storage"):
        raise ValueError("source must be 'foxglove' or 'object_storage'")
    if source == "foxglove" and object_store_factory is not None:
        raise ValueError("object_store_factory requires source='object_storage'")
    if not dataset_id:
        raise ValueError("dataset_id must be nonempty")
    if version < 1:
        raise ValueError("version must be a positive integer")
    selected_topics = tuple(dict.fromkeys(topics))
    # An empty topic list would read every topic.
    if isinstance(topics, str) or not selected_topics:
        raise ValueError("topics must be a nonempty sequence of topic names")
    make_client = client_factory if client_factory is not None else _make_client
    make_store: ObjectStoreFactory | None = None
    if source == "object_storage":
        make_store = (
            object_store_factory
            if object_store_factory is not None
            else _CloudObjectStore
        )
    episodes = []
    recordings: dict[str, tuple[str, ...]] = {}
    for episode in _version_episodes(
        dataset_id,
        version,
        make_client,
        include_recordings=make_store is not None or split_config is not None,
        include_properties=include_properties,
    ):
        if split_config is not None:
            recordings[episode["id"]] = _recording_ids(episode)
        episodes.append(
            _Episode(
                episode["id"],
                episode["start_time"],
                episode["end_time"],
                (
                    _locations(episode, validate=object_store_factory is None)
                    if make_store is not None
                    else ()
                ),
                version=episode["version"],
                # Episodes without properties may omit the field.
                properties=(
                    dict(episode.get("properties") or {})
                    if include_properties
                    else None
                ),
            )
        )
    # Make rank/worker partitioning independent of pagination order and sort ties.
    episodes.sort(key=lambda episode: episode.id)
    if split_config is not None and split is not None:
        selected = split_config._select(recordings, split)
        episodes = [episode for episode in episodes if episode.id in selected]
    return _Plan(
        dataset_id,
        version,
        selected_topics,
        tuple(episodes),
        make_client if make_store is None else None,
        make_store,
    )


def _version_episodes(
    dataset_id: str,
    version: int,
    client_factory: ClientFactory,
    *,
    include_recordings: bool,
    include_properties: bool = False,
) -> Iterator[Mapping[str, Any]]:
    if not dataset_id:
        raise ValueError("dataset_id must be nonempty")
    if version < 1:
        raise ValueError("version must be a positive integer")
    client = client_factory()
    info = client.get_dataset_version(dataset_id=dataset_id, version_number=version)
    if info["committed_at"] is None:
        raise ValueError("Dataset version must be committed")
    if info.get("has_missing_recordings", False):
        raise ValueError("Dataset version contains missing recordings")
    page = client.get_dataset_version_episodes(
        dataset_id=dataset_id,
        version_number=version,
        limit=_EPISODE_PAGE_SIZE,
        include_recordings=include_recordings,
        include_properties=include_properties,
    )
    for entry in page.auto_paging_iter():
        episode = entry["episode"]
        if "version" not in episode:
            raise ValueError(
                "Dataset membership must include pinned episode versions; "
                "upgrade to foxglove-client>=0.23.0"
            )
        if entry.get("has_missing_recordings", False):
            raise ValueError(f"Episode {episode['id']} contains missing recordings")
        yield episode


def _recording_ids(episode: Mapping[str, Any]) -> tuple[str, ...]:
    episode_recordings = episode.get("recordings")
    if not isinstance(episode_recordings, list) or not episode_recordings:
        raise ValueError(
            f"Episode {episode['id']} requires recording IDs for splitting"
        )
    recording_ids = []
    for recording in episode_recordings:
        recording_id = recording.get("id") if isinstance(recording, Mapping) else None
        if not isinstance(recording_id, str) or not recording_id:
            raise ValueError(f"Episode {episode['id']} has invalid recording IDs")
        recording_ids.append(recording_id)
    return tuple(recording_ids)


def inspect_splits(
    dataset_id: str,
    *,
    version: int,
    split_config: DatasetSplit,
    client_factory: ClientFactory | None = None,
) -> dict[SplitName, SplitSummary]:
    """Inspect train, validation, and test sizes using episode metadata only.

    Uses the same assignment as the PyTorch and Ray readers, without downloading
    messages or requiring either framework. Counts cover the whole committed
    version, before worker/rank partitioning. Proportions are probabilities per
    connected group.

    :param dataset_id: Required. ID of the dataset.
    :param version: Required. Number of a committed dataset version.
    :param split_config: Required. Split configuration to inspect.
    :param client_factory: Optional. Callable that returns the API client, for
        custom authentication or endpoints, as with ``read_dataset``. The default
        client uses ``FOXGLOVE_API_TOKEN``.
    :returns: The :class:`~foxglove.datasets.SplitSummary` of each split, keyed by
        ``"train"``, ``"validation"``, and ``"test"``.
    :raises ValueError: If an argument is invalid, ``FOXGLOVE_API_TOKEN`` is not
        set and no ``client_factory`` is given, the version is not committed or
        contains missing recordings, an episode has no recording IDs, or
        ``split_config.groups`` does not cover exactly the version's episodes.

    Example::

        from foxglove.datasets import DatasetSplit, inspect_splits

        summaries = inspect_splits(
            "dset_123", version=2, split_config=DatasetSplit()
        )
        train = summaries["train"]
        print(train.episode_count, train.group_count)
    """
    if not isinstance(split_config, DatasetSplit):
        raise ValueError("split_config must be a DatasetSplit")
    make_client = client_factory if client_factory is not None else _make_client
    recordings = {
        episode["id"]: _recording_ids(episode)
        for episode in _version_episodes(
            dataset_id, version, make_client, include_recordings=True
        )
    }
    return {
        name: SplitSummary(
            group_count=len(groups),
            episode_count=sum(len(episodes) for episodes in groups.values()),
        )
        for name, groups in split_config._partition(recordings).items()
    }


def _locations(
    episode: Mapping[str, Any], *, validate: bool = False
) -> tuple[ObjectLocation, ...]:
    recordings = episode.get("recordings")
    if not recordings:
        raise ValueError(
            f"Episode {episode['id']} has no recording locations; object storage "
            "requires a client with recording-location support and "
            "customer-managed indexed recordings"
        )
    locations = []
    for recording in recordings:
        location = recording.get("location")
        if not recording["available"]:
            raise ValueError(
                f"Recording {recording['id']} in episode {episode['id']} is not available"
            )
        if not location:
            raise ValueError(
                f"Recording {recording['id']} in episode {episode['id']} has no "
                "object location; use a client with "
                "recording-location support and customer-managed indexed recordings"
            )
        if not location.get("bucket") or not location.get("path"):
            raise ValueError(
                f"Recording {recording['id']} has an invalid object location"
            )
        object_location = ObjectLocation(
            bucket=location["bucket"],
            path=location["path"],
            azure_storage_account_name=location.get("azureStorageAccountName"),
            scheme=location.get("scheme"),
        )
        if validate:
            _validate_location(object_location)
        locations.append(object_location)
    return tuple(dict.fromkeys(locations))
