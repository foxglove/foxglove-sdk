import json
from collections.abc import Generator, Iterator
from datetime import datetime, timedelta, timezone
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan
from mcap.decoder import DecoderFactory
from mcap.records import Channel, Message, Schema


class Page:
    def auto_paging_iter(self) -> Iterator[dict[str, Any]]:
        # Deliberately non-sorted, spanning more than one yielded batch.
        for ids in (("c", "a"), ("d", "b")):
            for episode_id in ids:
                yield {
                    "has_missing_recordings": False,
                    "episode": {
                        "id": episode_id,
                        "start_time": datetime(2026, 1, 1, tzinfo=timezone.utc),
                        "end_time": datetime(2026, 1, 2, tzinfo=timezone.utc),
                        "metadata": {"label": episode_id},
                    },
                }


class Client:
    def __init__(self) -> None:
        self.calls: list[tuple[str, list[str]]] = []
        self.closed: list[str] = []
        self.time_ranges: list[tuple[datetime | None, datetime | None]] = []

    def get_dataset_version(
        self, *, dataset_id: str, version_number: int
    ) -> dict[str, Any]:
        assert (dataset_id, version_number) == ("dataset", 7)
        return {"committed_at": "2026-01-01", "has_missing_recordings": False}

    def get_dataset_version_episodes(
        self,
        *,
        dataset_id: str,
        version_number: int,
        limit: int,
        include_recordings: bool = False,
    ) -> Page:
        assert (dataset_id, version_number, limit) == ("dataset", 7, 2000)
        return Page()

    def iter_messages(
        self,
        *,
        episode_id: str,
        topics: list[str],
        decoder_factories: list[DecoderFactory] | None = None,
        start: datetime | None = None,
        end: datetime | None = None,
    ) -> Generator[tuple[Schema | None, Channel, Message, Any], None, None]:
        assert decoder_factories is None
        self.calls.append((episode_id, topics))
        self.time_ranges.append((start, end))
        try:
            yield (
                Schema(id=1, data=b"{}", encoding="jsonschema", name="Episode"),
                Channel(
                    id=1,
                    schema_id=1,
                    topic=topics[0],
                    message_encoding="json",
                    metadata={},
                ),
                Message(
                    channel_id=1,
                    log_time=0,
                    publish_time=0,
                    sequence=0,
                    data=json.dumps(episode_id).encode(),
                ),
                episode_id,
            )
            raise AssertionError("Read past the requested sample")
        finally:
            self.closed.append(episode_id)


def samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    for _schema, _channel, _message, decoded in episode.iter_messages():
        yield {"id": decoded}


@pytest.mark.parametrize("seconds", [0, 5])
def test_lookback_expands_request_without_changing_episode_window(seconds: int) -> None:
    client = Client()
    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    episode = plan.episodes[0]
    reader = EpisodeReader(episode, plan.topics, client)
    stream = reader.iter_messages(lookback=timedelta(seconds=seconds))
    next(stream)
    stream.close()
    assert client.time_ranges == [
        (
            (episode.start_time - timedelta(seconds=seconds), episode.end_time)
            if seconds
            else (None, None)
        )
    ]
    assert (reader.start_time, reader.end_time) == (
        episode.start_time,
        episode.end_time,
    )
    assert client.closed == [episode.id]


def test_rejects_negative_lookback_before_downloading() -> None:
    client = Client()
    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    reader = EpisodeReader(plan.episodes[0], plan.topics, client)
    with pytest.raises(ValueError, match="nonnegative"):
        next(reader.iter_messages(lookback=timedelta(seconds=-1)))
    assert not client.calls


@pytest.mark.parametrize("topics", [[], "camera"])
def test_rejects_implicit_all_topic_reads(topics: Any) -> None:
    with pytest.raises(ValueError, match="topics"):
        _plan("dataset", 7, topics, Client)


def test_rejects_invalid_version() -> None:
    with pytest.raises(ValueError, match="version"):
        _plan("dataset", 0, ["/camera"], Client)


@pytest.mark.parametrize(
    "info, message",
    [
        ({"committed_at": None}, "committed"),
        ({"committed_at": "date", "has_missing_recordings": True}, "missing"),
    ],
)
def test_rejects_editable_or_incomplete_version(
    monkeypatch: pytest.MonkeyPatch, info: dict[str, Any], message: str
) -> None:
    monkeypatch.setattr(Client, "get_dataset_version", lambda *args, **kwargs: info)
    with pytest.raises(ValueError, match=message):
        _plan("dataset", 7, ["/camera"], Client)


def test_rechecks_missing_recordings_on_episode_entries(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    entry = next(Page().auto_paging_iter())
    entry["has_missing_recordings"] = True
    monkeypatch.setattr(Page, "auto_paging_iter", lambda self: iter([entry]))
    with pytest.raises(ValueError, match="Episode c.*missing"):
        _plan("dataset", 7, ["/camera"], Client)


def test_empty_plan_yields_no_samples(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(Page, "auto_paging_iter", lambda self: iter([]))
    plan = _plan("dataset", 7, ["/camera"], Client)
    assert list(plan.read(plan.episodes, samples)) == []


@pytest.mark.parametrize("outcome", ["complete", "cancel", "error"])
def test_callback_lifecycle_closes_resources(outcome: str) -> None:
    client = Client()
    events = []
    readers = []
    retained = []

    def read_episode(episode: EpisodeReader) -> Iterator[tuple[str, str]]:
        readers.append(episode)
        messages = episode.iter_messages()
        retained.append(messages)
        next(messages)
        try:
            if outcome == "error":
                raise ValueError("bad schema")
            yield (episode.id, episode.metadata["label"])
        finally:
            events.append(episode.id)

    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    stream = plan.read(plan.episodes, read_episode)
    if outcome == "error":
        with pytest.raises(
            RuntimeError, match="episode a.*dataset dataset version 7"
        ) as error:
            next(stream)
        assert isinstance(error.value.__cause__, ValueError)
        expected = ["a"]
    elif outcome == "cancel":
        assert next(stream) == ("a", "a")
        stream.close()
        expected = ["a"]
    else:
        assert list(stream) == [(name, name) for name in ("a", "b", "c", "d")]
        expected = ["a", "b", "c", "d"]
    assert events == expected
    assert client.closed == expected
    for reader in readers:
        with pytest.raises(RuntimeError, match="Episode reader is closed"):
            next(reader.iter_messages())
    for messages in retained:
        assert list(messages) == []


@pytest.mark.parametrize("topics", [[], "/camera", ["/outside"]])
def test_reader_rejects_invalid_topic_subset(topics: Any) -> None:
    client = Client()
    plan = _plan("dataset", 7, ["/camera", "/other"], lambda: client)
    reader = EpisodeReader(plan.episodes[0], plan.topics, client)
    with pytest.raises(ValueError, match="topics"):
        next(reader.iter_messages(topics=topics))
    assert not client.calls
