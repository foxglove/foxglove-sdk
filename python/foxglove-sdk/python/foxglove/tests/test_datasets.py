from collections.abc import Generator, Iterator
from datetime import datetime, timezone
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan


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
        self, *, dataset_id: str, version_number: int
    ) -> Page:
        assert (dataset_id, version_number) == ("dataset", 7)
        return Page()

    def iter_messages(
        self, *, episode_id: str, topics: list[str]
    ) -> Generator[Any, None, None]:
        self.calls.append((episode_id, topics))
        try:
            yield episode_id
            raise AssertionError("Read past the requested sample")
        finally:
            self.closed.append(episode_id)


def samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    for message in episode.iter_messages():
        yield {"id": message}


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
        yield {"id": next(messages), "label": episode.metadata["label"]}

    plan = _plan("dataset", 7, ["/camera"], lambda: client)
    assert list(plan.read(plan.episodes, first)) == [
        {"id": name, "label": name} for name in ("a", "b", "c", "d")
    ]
    assert client.closed == ["a", "b", "c", "d"]


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


@pytest.mark.parametrize("topics", [[], "camera", [""], [None], iter(["/camera"])])
def test_rejects_implicit_all_topic_reads(topics: Any) -> None:
    with pytest.raises(ValueError, match="topics"):
        _plan("dataset", 7, topics, Client)


@pytest.mark.parametrize("version", [0, -1, True, "7"])
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
