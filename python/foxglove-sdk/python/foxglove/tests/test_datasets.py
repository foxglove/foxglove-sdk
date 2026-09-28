import json
from collections.abc import Generator, Iterator
from datetime import datetime, timezone
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan
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

    def get_dataset_version(
        self, *, dataset_id: str, version_number: int
    ) -> dict[str, Any]:
        assert (dataset_id, version_number) == ("dataset", 7)
        return {"committed_at": "2026-01-01", "has_missing_recordings": False}

    def get_dataset_version_episodes(
        self, *, dataset_id: str, version_number: int, limit: int
    ) -> Page:
        assert (dataset_id, version_number, limit) == ("dataset", 7, 2000)
        return Page()

    def iter_messages(
        self, *, episode_id: str, topics: list[str], **kwargs: Any
    ) -> Generator[tuple[Schema | None, Channel, Message, Any], None, None]:
        assert kwargs == {}
        self.calls.append((episode_id, topics))
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


def test_metadata_only_plan_and_lazy_topic_filtered_reads() -> None:
    client = Client()
    plan = _plan("dataset", 7, ["/camera", "/camera"], lambda: client)
    assert [episode.id for episode in plan.episodes] == ["a", "b", "c", "d"]
    stream = plan.read(plan.episodes, samples)
    assert client.calls == []
    assert next(stream) == {"id": "a"}
    assert client.calls == [("a", ["/camera"])]
    stream.close()
    assert client.closed == ["a"]


def test_callback_can_stop_early_and_retain_message_iterator() -> None:
    client = Client()
    retained = []

    def first(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        messages = episode.iter_messages()
        retained.append(messages)
        yield {"id": next(messages)[3], "label": episode.metadata["label"]}

    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    assert list(plan.read(plan.episodes, first)) == [
        {"id": name, "label": name} for name in ("a", "b", "c", "d")
    ]
    assert client.closed == ["a", "b", "c", "d"]


def test_episode_metadata_is_read_only() -> None:
    def mutate(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        episode.metadata["label"] = "changed"  # type: ignore[index]
        yield {}

    plan = _plan("dataset", 7, ["/camera"], Client)
    with pytest.raises(RuntimeError) as error:
        list(plan.read(plan.episodes, mutate))
    assert isinstance(error.value.__cause__, TypeError)
    assert plan.episodes[0].metadata == {"label": "a"}


def test_callback_error_closes_stream_and_adds_episode_context() -> None:
    client = Client()

    def fail(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        messages = episode.iter_messages()
        next(messages)
        raise ValueError("bad schema")

    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    with pytest.raises(
        RuntimeError, match="episode a.*dataset dataset version 7"
    ) as error:
        list(plan.read(plan.episodes, fail))
    assert isinstance(error.value.__cause__, ValueError)
    assert client.closed == ["a"]


@pytest.mark.parametrize("topics", [[], "camera", iter([])])
def test_rejects_implicit_all_topic_reads(topics: Any) -> None:
    with pytest.raises(ValueError, match="topics"):
        _plan("dataset", 7, topics, Client)


@pytest.mark.parametrize("version", [0, -1])
def test_rejects_invalid_version(version: Any) -> None:
    with pytest.raises(ValueError, match="version"):
        _plan("dataset", version, ["/camera"], Client)


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


def test_omitted_missing_recordings_flags_do_not_block_planning(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(
        Client,
        "get_dataset_version",
        lambda *args, **kwargs: {"committed_at": "2026-01-01"},
    )
    entries = list(Page().auto_paging_iter())
    for entry in entries:
        del entry["has_missing_recordings"]
    monkeypatch.setattr(Page, "auto_paging_iter", lambda self: iter(entries))

    plan = _plan("dataset", 7, ["/camera"], Client)

    assert [episode.id for episode in plan.episodes] == ["a", "b", "c", "d"]


def test_empty_plan_does_not_create_worker_client(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(Page, "auto_paging_iter", lambda self: iter([]))
    created = []

    def factory() -> Client:
        created.append(True)
        return Client()

    plan = _plan("dataset", 7, ["/camera"], factory)
    assert list(plan.read(plan.episodes, samples)) == []
    assert len(created) == 1


@pytest.mark.parametrize("cancel", [False, True])
def test_callback_cleanup_runs_before_episode_streams_close(cancel: bool) -> None:
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
            yield (episode.id, episode.metadata["label"])
        finally:
            assert episode.id not in client.closed
            events.append(episode.id)

    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    stream = plan.read(plan.episodes, read_episode)
    if cancel:
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
