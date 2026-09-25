"""Shared planning and streaming, using the Foxglove Python API client."""

from __future__ import annotations

from collections.abc import Callable, Generator, Iterable, Iterator, Mapping, Sequence
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Protocol


class _MessageIterator(Protocol):
    def __iter__(self) -> Iterator[Any]: ...

    def __next__(self) -> Any: ...

    def close(self) -> None: ...


class _Page(Protocol):
    def auto_paging_iter(self) -> Iterator[dict[str, Any]]: ...


class _Client(Protocol):
    def get_dataset_version(
        self, *, dataset_id: str, version_number: int
    ) -> Mapping[str, Any]: ...

    def get_dataset_version_episodes(
        self, *, dataset_id: str, version_number: int
    ) -> _Page: ...

    def iter_messages(
        self, *, episode_id: str, topics: list[str]
    ) -> _MessageIterator: ...


ClientFactory = Callable[[], _Client]
ReadEpisode = Callable[["EpisodeReader"], Iterable[dict[str, Any]]]


@dataclass(frozen=True)
class _Episode:
    id: str
    start_time: datetime
    end_time: datetime
    metadata: dict[str, Any]


class EpisodeReader:
    """An episode scoped to the topics selected by ``read_dataset``.

    Instances are supplied to the customer's ``read_episode`` callback and remain
    usable only until that callback's iterator finishes or is closed. Message tuples
    are the client's ``(schema, channel, message, decoded_message)`` values. Configure
    custom client decoders in the worker's client factory when needed.
    """

    def __init__(
        self, episode: _Episode, topics: tuple[str, ...], client: _Client
    ) -> None:
        self._episode = episode
        self._topics = topics
        self._client = client
        self._streams: list[_MessageIterator] = []
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
        """Customer-defined episode metadata."""
        return self._episode.metadata

    def iter_messages(self) -> Generator[Any, None, None]:
        """Stream the selected topics in log-time order without buffering the episode.

        Each invocation opens a new stream on first iteration. Repeated invocations
        redownload data. Active streams are closed when the callback ends.
        """
        if self._closed:
            raise RuntimeError("Episode reader is closed")
        stream = self._client.iter_messages(
            episode_id=self.id, topics=list(self._topics)
        )
        self._streams.append(stream)
        try:
            yield from stream
        finally:
            stream.close()
            self._streams.remove(stream)

    def _close(self) -> None:
        self._closed = True
        for stream in self._streams:
            stream.close()


@dataclass(frozen=True)
class _Plan:
    dataset_id: str
    version: int
    topics: tuple[str, ...]
    episodes: tuple[_Episode, ...]
    client_factory: ClientFactory

    def read(
        self, episodes: Sequence[_Episode], read_episode: ReadEpisode
    ) -> Generator[dict[str, Any], None, None]:
        if not episodes:
            return
        client = self.client_factory()
        for episode in episodes:
            reader = EpisodeReader(episode, self.topics, client)
            samples = None
            try:
                samples = iter(read_episode(reader))
                for sample in samples:
                    if not isinstance(sample, dict):
                        raise TypeError("read_episode must yield dictionaries")
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
    if isinstance(version, bool) or not isinstance(version, int) or version < 1:
        raise ValueError("version must be a positive integer")
    if (
        isinstance(topics, (str, bytes))
        or not isinstance(topics, Sequence)
        or not topics
        or any(not isinstance(topic, str) or not topic for topic in topics)
    ):
        raise ValueError("topics must be a nonempty sequence of nonempty topic names")
    selected_topics = tuple(dict.fromkeys(topics))
    client = client_factory()
    if not hasattr(client, "get_dataset_version"):
        raise RuntimeError(
            "Dataset loading requires foxglove-client>=0.20.0. "
            "Upgrade your client installation."
        )
    info = client.get_dataset_version(dataset_id=dataset_id, version_number=version)
    if info["committed_at"] is None:
        raise ValueError("Dataset version must be committed")
    if info.get("has_missing_recordings", False):
        raise ValueError("Dataset version contains missing recordings")
    episodes = []
    page = client.get_dataset_version_episodes(
        dataset_id=dataset_id, version_number=version
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
