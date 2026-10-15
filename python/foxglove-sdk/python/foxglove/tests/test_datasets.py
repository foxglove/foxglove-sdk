from collections.abc import Iterator
from datetime import timedelta
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan
from foxglove.datasets.storage import _decoder_not_found_hint, _missing_decoder_message
from mcap.exceptions import DecoderNotFoundError

from .datasets_helpers import Client, Page


def samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    for _schema, _channel, _message, decoded in episode.iter_messages():
        yield {"id": decoded}


@pytest.mark.parametrize(
    "schema_encoding,message_encoding,expected",
    [
        (
            "protobuf",
            "protobuf",
            "No decoder for schema encoding 'protobuf'; install mcap-protobuf-support",
        ),
        (
            "ros1msg",
            "ros1",
            "No decoder for schema encoding 'ros1msg' "
            "(message encoding 'ros1'); install mcap-ros1-support",
        ),
        (
            "ros2msg",
            "cdr",
            "No decoder for schema encoding 'ros2msg' "
            "(message encoding 'cdr'); install mcap-ros2-support",
        ),
        (
            "omgidl",
            "cdr",
            "No decoder for schema encoding 'omgidl' (message encoding 'cdr')",
        ),
    ],
)
def test_missing_decoder_message_names_known_encoding_and_package(
    schema_encoding: str, message_encoding: str, expected: str
) -> None:
    assert _missing_decoder_message(schema_encoding, message_encoding) == expected
    error = DecoderNotFoundError(
        "no decoder factory supplied for message encoding "
        f"{message_encoding}, schema Schema(id=1, data=b'', "
        f"encoding='{schema_encoding}', name='Example')"
    )
    assert _decoder_not_found_hint(error) == expected


def test_missing_decoder_message_uses_message_encoding_without_schema() -> None:
    assert _missing_decoder_message(None, "cdr") == (
        "No decoder for message encoding 'cdr'; install mcap-ros2-support"
    )


def test_missing_decoder_error_names_schema_encoding_and_package(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    error = DecoderNotFoundError(
        "no decoder factory supplied for message encoding protobuf, "
        "schema Schema(id=1, data=b'', encoding='protobuf', "
        "name='foxglove.CompressedImage')"
    )
    assert _decoder_not_found_hint(error) == (
        "No decoder for schema encoding 'protobuf'; install mcap-protobuf-support"
    )

    def iter_messages(*args: Any, **kwargs: Any) -> Iterator[Any]:
        raise error

    monkeypatch.setattr(Client, "iter_messages", iter_messages)
    plan = _plan("dataset", 7, ["/camera"], Client)
    with pytest.raises(
        RuntimeError,
        match=(
            r"Failed to read episode a in dataset dataset version 7: "
            r"No decoder for schema encoding 'protobuf'; install mcap-protobuf-support"
        ),
    ) as wrapped:
        list(plan.read(plan.episodes, samples))
    assert wrapped.value.__cause__ is error


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
