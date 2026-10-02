"""Shared planning and streaming, using the Foxglove Python API client."""

from __future__ import annotations

from collections.abc import Callable, Generator, Iterable, Iterator, Mapping, Sequence
from contextlib import ExitStack
from dataclasses import dataclass
from datetime import datetime, timedelta
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, Protocol, TypeVar

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
        self, *, dataset_id: str, version_number: int, limit: int
    ) -> _Page: ...

    def iter_messages(
        self,
        *,
        episode_id: str,
        topics: list[str],
        decoder_factories: list[DecoderFactory] | None = None,
        start: datetime | None = None,
        end: datetime | None = None,
    ) -> Generator[tuple[Schema | None, Channel, Message, Any], None, None]: ...


ClientFactory = Callable[[], _Client]


@dataclass(frozen=True)
class _Episode:
    id: str
    start_time: datetime
    end_time: datetime
    metadata: dict[str, Any]


class EpisodeReader:
    """An episode scoped to the topics selected by ``read_dataset``.

    .. warning::

        This API is experimental and unstable. It may change in backward-incompatible
        ways as we continue development and incorporate user feedback.

    Instances are supplied to the user's ``read_episode`` callback and remain
    usable only until that callback's iterator finishes or is closed. Message tuples
    are the client's ``(schema, channel, message, decoded_message)`` values. Create
    custom message decoders inside the callback and pass them to :meth:`iter_messages`.
    Media decoding (such as H.264 to images) is a separate processing step.
    """

    def __init__(
        self, episode: _Episode, topics: tuple[str, ...], client: _Client
    ) -> None:
        self._episode = episode
        self._topics = topics
        self._client = client
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
    def metadata(self) -> Mapping[str, Any]:
        """User-defined episode metadata."""
        return MappingProxyType(self._episode.metadata)

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

        :param decoder_factories: MCAP decoder factories used for message
            deserialization. ``None`` uses the client's default decoders; an explicit
            list replaces them. Construct factories inside the callback so their
            state stays local to the worker and episode. This does not decode media
            payloads into images.
        :param topics: Optional nonempty subset of the dataset topics. Filtering
            happens on the server; separate calls open separate downloads.
        :param lookback: Additional history to request before the episode start,
            for stateful media decoding. Must be nonnegative. Only recordings
            attached to the episode are searched; history may be unavailable.
            Consumers must exclude lookback messages from their training samples.
        :returns: Tuples of schema (possibly ``None``), channel, raw MCAP message,
            and decoded message payload.
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
        time_range = (
            {"start": self.start_time - lookback, "end": self.end_time}
            if lookback
            else {}
        )
        stream = self._client.iter_messages(
            episode_id=self.id,
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
    client_factory: ClientFactory

    def read(
        self, episodes: Sequence[_Episode], read_episode: ReadEpisode[_T]
    ) -> Generator[_T, None, None]:
        if not episodes:
            return
        client = self.client_factory()
        for episode in episodes:
            reader = EpisodeReader(episode, self.topics, client)
            samples = None
            try:
                samples = iter(read_episode(reader))
                for sample in samples:
                    yield sample
            except Exception as error:
                raise RuntimeError(
                    f"Failed to read episode {episode.id} in dataset "
                    f"{self.dataset_id} version {self.version}"
                ) from error
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
    client_factory: ClientFactory,
) -> _Plan:
    if not dataset_id:
        raise ValueError("dataset_id must be nonempty")
    if version < 1:
        raise ValueError("version must be a positive integer")
    selected_topics = tuple(dict.fromkeys(topics))
    # An empty topic list would read every topic.
    if isinstance(topics, str) or not selected_topics:
        raise ValueError("topics must be a nonempty sequence of topic names")
    client = client_factory()
    info = client.get_dataset_version(dataset_id=dataset_id, version_number=version)
    if info["committed_at"] is None:
        raise ValueError("Dataset version must be committed")
    if info.get("has_missing_recordings", False):
        raise ValueError("Dataset version contains missing recordings")
    episodes = []
    page = client.get_dataset_version_episodes(
        dataset_id=dataset_id, version_number=version, limit=_EPISODE_PAGE_SIZE
    )
    for entry in page.auto_paging_iter():
        episode = entry["episode"]
        if entry.get("has_missing_recordings", False):
            raise ValueError(f"Episode {episode['id']} contains missing recordings")
        episodes.append(
            _Episode(
                episode["id"],
                episode["start_time"],
                episode["end_time"],
                dict(episode["metadata"]),
            )
        )
    # Make rank/worker partitioning independent of pagination order and sort ties.
    episodes.sort(key=lambda episode: episode.id)
    return _Plan(
        dataset_id,
        version,
        selected_topics,
        tuple(episodes),
        client_factory,
    )
