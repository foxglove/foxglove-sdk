import io
import json
from collections.abc import Callable, Iterator
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan

client_module = pytest.importorskip("foxglove.client")


@pytest.mark.parametrize("custom_decoding", [False, True])
def test_real_client_pagination_filtering_and_incremental_mcap(
    monkeypatch: pytest.MonkeyPatch,
    custom_decoding: bool,
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
        "dataset", 7, ["/camera", "/camera"], lambda: client_module.Client(token="test")
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
            decoder_factories=decoders
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
