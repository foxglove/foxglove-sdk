import io
import json
from collections.abc import Iterator
from typing import Any

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _plan

client_module = pytest.importorskip("foxglove.client")


def test_real_client_pagination_filtering_and_incremental_mcap(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import requests
    from mcap.writer import CompressionType, Writer

    output = io.BytesIO()
    writer = Writer(output, chunk_size=1, compression=CompressionType.NONE)
    writer.start()
    schema = writer.register_schema("Measurement", "jsonschema", b"{}")
    channel = writer.register_channel("/camera", "json", schema)
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
            cursor = kwargs["params"].get("cursor")
            payload = {
                "episodes": [
                    {
                        "addedAt": stamp,
                        "addedInVersion": 7,
                        "hasMissingRecordings": False,
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
                "hasMissingRecordings": False,
            }
        response._content = json.dumps(payload).encode()
        return response

    monkeypatch.setattr(requests.Session, "request", request)
    plan = _plan("dataset", 7, ["/camera"], lambda: client_module.Client(token="test"))
    assert [episode.id for episode in plan.episodes] == ["a", "b"]
    assert len(calls) == 3
    assert opened == []

    def samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
        for _schema, _channel, _message, decoded in episode.iter_messages():
            yield decoded

    stream = plan.read(plan.episodes, samples)
    assert next(stream) == {"value": 0}
    assert opened[0].tell() < len(data)
    stream.close()
    assert opened[0].closed
    assert len(calls) == 5
