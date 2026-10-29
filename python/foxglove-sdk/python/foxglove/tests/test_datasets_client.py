import io
import json
from collections.abc import Callable, Iterator
from datetime import datetime, timedelta, timezone
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan

client_module = pytest.importorskip("foxglove.client")


@pytest.mark.parametrize("custom_decoding", [False, True])
@pytest.mark.parametrize("lookback", [timedelta(0), timedelta(seconds=5)])
def test_real_client_pagination_filtering_and_incremental_mcap(
    monkeypatch: pytest.MonkeyPatch,
    custom_decoding: bool,
    lookback: timedelta,
) -> None:
    import requests
    from mcap.decoder import DecoderFactory
    from mcap.records import Schema
    from mcap.writer import CompressionType, Writer

    decoded_values = []

    class CustomDecoderFactory(DecoderFactory):
        def decoder_for(
            self, message_encoding: str, schema: Schema | None
        ) -> Callable[[bytes], Any] | None:
            assert message_encoding == "custom"
            assert schema is not None and schema.name == "Measurement"

            def decode(data: bytes) -> dict[str, Any]:
                payload = json.loads(data)
                decoded_values.append(payload["value"])
                return {"value": payload["value"] + 10}

            return decode

    output = io.BytesIO()
    writer = Writer(output, chunk_size=1, compression=CompressionType.NONE)
    writer.start()
    schema = writer.register_schema("Measurement", "jsonschema", b"{}")
    channel = writer.register_channel(
        "/camera", "custom" if custom_decoding else "json", schema
    )
    for index in range(100):
        writer.add_message(channel, index, json.dumps({"value": index}).encode(), index)
    writer.finish()
    data = output.getvalue()
    opened: list[io.BytesIO] = []
    calls: list[tuple[str, str, dict[str, Any]]] = []

    class Stream(io.BytesIO):
        def seekable(self) -> bool:
            return False

    def request(
        session: requests.Session, method: str, url: str, **kwargs: Any
    ) -> requests.Response:
        calls.append((method.upper(), url, kwargs))
        response = requests.Response()
        response.status_code = 200
        response.url = url
        if url == "https://signed.example/data":
            raw = Stream(data)
            opened.append(raw)
            response.raw = raw
            return response
        stamp = "2026-01-01T00:00:00Z"
        payload: dict[str, Any]
        if url.endswith("/episodes"):
            assert kwargs["params"]["limit"] == 2000
            cursor = kwargs["params"].get("cursor")
            assert cursor in (None, "second")
            payload = {
                "episodes": [
                    {
                        "addedAt": stamp,
                        "addedInVersion": 7,
                        "episode": {
                            "version": 2,
                            "latestVersion": 3,
                            "id": "b" if cursor else "a",
                            "projectId": "project",
                            "startTime": stamp,
                            "endTime": stamp,
                            "metadata": {},
                            "createdAt": stamp,
                        },
                    }
                ]
            }
            if not cursor:
                response.headers["fg-pagination-next-cursor"] = "second"
        elif url.endswith("/data/stream"):
            assert kwargs["json"]["topics"] == ["/camera"]
            assert kwargs["json"]["episodeId"] == "a"
            assert kwargs["json"]["episodeVersion"] == 2
            if lookback:
                assert datetime.fromisoformat(kwargs["json"]["start"]) == datetime(
                    2025, 12, 31, 23, 59, 55, tzinfo=timezone.utc
                )
                assert datetime.fromisoformat(kwargs["json"]["end"]) == datetime(
                    2026, 1, 1, tzinfo=timezone.utc
                )
            else:
                assert "start" not in kwargs["json"] and "end" not in kwargs["json"]
            payload = {"link": "https://signed.example/data"}
        else:
            assert url.endswith("/datasets/dataset/versions/7")
            payload = {
                "versionNumber": 7,
                "committedAt": stamp,
                "createdAt": stamp,
                "episodeCount": 2,
                "addedEpisodeCount": 2,
                "removedEpisodeCount": 0,
            }
        response._content = json.dumps(payload).encode()
        return response

    monkeypatch.setattr(requests.Session, "request", request)
    plan = _plan(
        "dataset",
        7,
        ["/camera", "/camera", "/other"],
        lambda: client_module.Client(token="test"),
    )
    assert [episode.id for episode in plan.episodes] == ["a", "b"]
    episode_requests = [kwargs for _, url, kwargs in calls if url.endswith("/episodes")]
    assert [request["params"].get("cursor") for request in episode_requests] == [
        None,
        "second",
    ]
    assert opened == []

    def samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        decoders = [CustomDecoderFactory()] if custom_decoding else None
        for schema, channel, message, decoded in episode.iter_messages(
            decoder_factories=decoders, lookback=lookback, topics=["/camera"]
        ):
            assert schema is not None and schema.name == "Measurement"
            assert channel.topic == "/camera"
            assert message.log_time == message.publish_time
            assert json.loads(message.data)["value"] == message.log_time
            yield decoded

    stream = plan.read(plan.episodes, samples)
    offset = 10 if custom_decoding else 0
    assert [next(stream) for _ in range(3)] == [
        {"value": index + offset} for index in range(3)
    ]
    assert decoded_values == ([0, 1, 2] if custom_decoding else [])
    assert opened[0].tell() < len(data)
    stream.close()
    assert opened[0].closed


@pytest.mark.parametrize("builtin", [False, True])
def test_real_client_object_storage_locations_and_cursor_pagination(
    monkeypatch: pytest.MonkeyPatch,
    builtin: bool,
) -> None:
    import requests
    from foxglove.datasets.storage import ObjectLocation

    from .datasets_helpers import Store, mcap_bytes

    locations = [
        ObjectLocation("bucket", "prefix/s3.mcap", scheme="s3"),
        ObjectLocation("container", "prefix/azure.mcap", "account", scheme="az"),
    ]
    store = Store(
        {
            location: mcap_bytes([("/camera", 0, {"value": index})])
            for index, location in enumerate(locations)
        }
    )
    calls: list[tuple[str, dict[str, Any]]] = []

    def request(
        session: requests.Session, method: str, url: str, **kwargs: Any
    ) -> requests.Response:
        assert method.upper() == "GET"
        calls.append((url, kwargs))
        stamp = "2026-01-01T00:00:00Z"
        response = requests.Response()
        response.status_code = 200
        response.url = url
        payload: dict[str, Any]
        if url.endswith("/episodes"):
            assert kwargs["params"]["include"] == "recordings"
            assert kwargs["params"]["limit"] == 2000
            cursor = kwargs["params"].get("cursor")
            assert cursor in (None, "second")
            index = 1 if cursor else 0
            location = locations[index]
            raw_location: dict[str, Any] = {
                "bucket": location.bucket,
                "path": location.path,
                "scheme": location.scheme,
            }
            if location.azure_storage_account_name:
                raw_location["azureStorageAccountName"] = (
                    location.azure_storage_account_name
                )
            payload = {
                "episodes": [
                    {
                        "addedAt": stamp,
                        "addedInVersion": 7,
                        "episode": {
                            "version": 2,
                            "latestVersion": 3,
                            "id": "a" if cursor else "b",
                            "projectId": "project",
                            "startTime": stamp,
                            "endTime": stamp,
                            "metadata": {},
                            "createdAt": stamp,
                            "recordings": [
                                {
                                    "id": f"recording_{index}",
                                    "path": "display-name.mcap",
                                    "location": raw_location,
                                    "start": stamp,
                                    "end": stamp,
                                    "available": True,
                                    "resolvable": True,
                                }
                            ],
                        },
                    }
                ]
            }
            if not cursor:
                response.headers["fg-pagination-next-cursor"] = "second"
        else:
            assert url.endswith("/datasets/dataset/versions/7")
            payload = {
                "versionNumber": 7,
                "committedAt": stamp,
                "createdAt": stamp,
                "episodeCount": 2,
                "addedEpisodeCount": 2,
                "removedEpisodeCount": 0,
            }
        response._content = json.dumps(payload).encode()
        return response

    monkeypatch.setattr(requests.Session, "request", request)
    monkeypatch.setenv("FOXGLOVE_API_TOKEN", "test")
    monkeypatch.setattr("foxglove.datasets.reader._CloudObjectStore", lambda: store)
    plan = _plan(
        "dataset",
        7,
        ["/camera"],
        None if builtin else lambda: client_module.Client(token="test"),
        None if builtin else lambda: store,
        source="object_storage",
    )
    assert [episode.locations for episode in plan.episodes] == [
        (locations[1],),
        (locations[0],),
    ]
    assert [
        call["params"].get("cursor") for url, call in calls if url.endswith("/episodes")
    ] == [
        None,
        "second",
    ]
    assert store.files == []
    planning_requests = len(calls)

    def samples(reader: EpisodeReader) -> Iterator[dict[str, Any]]:
        for _, _, _, decoded in reader.iter_messages():
            yield decoded

    assert list(plan.read(plan.episodes, samples)) == [{"value": 1}, {"value": 0}]
    assert store.locations == [locations[1], locations[0]]
    assert all(file.closed for file in store.files)
    assert len(calls) == planning_requests == 3


@pytest.mark.parametrize("edit", ["window", "recordings"])
@pytest.mark.parametrize("lookback", [timedelta(0), timedelta(seconds=5)])
def test_committed_dataset_streams_pinned_episode_after_edits(
    monkeypatch: pytest.MonkeyPatch, edit: str, lookback: timedelta
) -> None:
    import pickle

    import requests

    from .datasets_helpers import START, START_NS, mcap_bytes

    committed_end = START + timedelta(seconds=256)
    snapshots: dict[int, dict[str, Any]] = {
        1: {"end": committed_end, "recording": "original"}
    }
    latest_version = 1
    committed_version: int | None = None
    requests_seen: list[dict[str, Any]] = []
    streams: dict[str, bytes] = {}
    recordings: dict[str, list[tuple[str, int, Any]]] = {
        "original": [
            ("/camera", index * 1_000_000_000, index) for index in range(-5, 257)
        ]
        + [("/other", 0, "unselected")],
        "replacement": [
            ("/camera", index * 1_000_000_000, 1000 + index) for index in range(157)
        ],
    }
    stamp = START.isoformat()
    dataset_version = {
        "versionNumber": 7,
        "committedAt": stamp,
        "createdAt": stamp,
        "episodeCount": 1,
        "addedEpisodeCount": 1,
        "removedEpisodeCount": 0,
    }

    def request(
        session: requests.Session, method: str, url: str, **kwargs: Any
    ) -> requests.Response:
        nonlocal latest_version, committed_version
        response = requests.Response()
        response.status_code = 200
        response.url = url
        payload: dict[str, Any]
        if url in streams:
            response.raw = io.BytesIO(streams[url])
            return response
        if url.endswith("/commit"):
            assert method.upper() == "POST"
            committed_version = latest_version
            payload = {"committed": dataset_version, "editableVersionNumber": 8}
        elif url.endswith("/v1/episodes"):
            assert method.upper() == "PATCH"
            change = kwargs["json"]["episodes"][0]
            assert change["id"] == "episode"
            snapshot = dict(snapshots[latest_version])
            if "endTime" in change:
                snapshot["end"] = datetime.fromisoformat(change["endTime"])
            if "recordings" in change:
                assert change["recordings"] == {
                    "add": ["replacement"],
                    "remove": ["original"],
                }
                snapshot["recording"] = "replacement"
            latest_version += 1
            snapshots[latest_version] = snapshot
            payload = {"episodes": [{"id": "episode", "version": latest_version}]}
        elif url.endswith("/episodes"):
            assert committed_version is not None
            snapshot = snapshots[committed_version]
            payload = {
                "episodes": [
                    {
                        "addedAt": stamp,
                        "addedInVersion": 7,
                        "episode": {
                            "id": "episode",
                            "projectId": "project",
                            "version": committed_version,
                            "latestVersion": latest_version,
                            "startTime": stamp,
                            "endTime": snapshot["end"].isoformat(),
                            "metadata": {"recording": snapshot["recording"]},
                            "createdAt": stamp,
                        },
                    }
                ]
            }
        elif url.endswith("/data/stream"):
            params = kwargs["json"]
            requests_seen.append(params)
            snapshot = snapshots[params.get("episodeVersion", latest_version)]
            start = (
                datetime.fromisoformat(params["start"]) if "start" in params else START
            )
            end = (
                datetime.fromisoformat(params["end"])
                if "end" in params
                else snapshot["end"]
            )
            selected = [
                message
                for message in recordings[snapshot["recording"]]
                if message[0] in params["topics"]
                and start <= START + timedelta(microseconds=message[1] // 1000) <= end
            ]
            link = f"https://signed.example/{len(streams)}"
            streams[link] = mcap_bytes(selected)
            payload = {"link": link}
        else:
            assert committed_version is not None
            payload = dataset_version
        response._content = json.dumps(payload).encode()
        return response

    monkeypatch.setattr(requests.Session, "request", request)

    def make_client() -> Any:
        return client_module.Client(token="test")

    client = make_client()
    client.commit_dataset(dataset_id="dataset")
    plan = _plan("dataset", 7, ["/camera", "/other"], make_client)
    change: dict[str, Any] = {"id": "episode"}
    if edit == "window":
        change["end_time"] = START + timedelta(seconds=156)
    else:
        change["recordings"] = {"add": ["replacement"], "remove": ["original"]}
    client.update_episodes(episodes=[change])

    def samples(reader: EpisodeReader) -> Iterator[int]:
        assert reader.start_time == START
        assert reader.end_time == committed_end
        assert reader.metadata == {"recording": "original"}
        for _, channel, message, decoded in reader.iter_messages(
            topics=["/camera"], lookback=lookback
        ):
            assert channel.topic == "/camera"
            assert message.log_time == START_NS + decoded * 1_000_000_000
            if message.log_time >= START_NS:
                yield decoded

    for _epoch in range(2):
        # Episode snapshots must retain their pin when transported to workers.
        episodes = pickle.loads(pickle.dumps(plan.episodes))
        assert list(plan.read(episodes, samples)) == list(range(257))
    replanned = _plan("dataset", 7, ["/camera"], make_client)
    assert list(replanned.read(replanned.episodes, samples)) == list(range(257))
    assert all(params["episodeVersion"] == 1 for params in requests_seen)
    assert all(params["topics"] == ["/camera"] for params in requests_seen)
    if lookback:
        assert all(
            datetime.fromisoformat(params["start"]) == START - lookback
            for params in requests_seen
        )
    latest_values = [
        decoded
        for _, _, _, decoded in client.iter_messages(
            episode_id="episode", topics=["/camera"]
        )
    ]
    assert latest_values == list(range(157) if edit == "window" else range(1000, 1157))
