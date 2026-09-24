import json
import re
import sys
import threading
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse

import pyarrow as pa
import pyarrow.parquet as pq
import pytest
import upload

API_KEY = "fox_sk_test"
TASK = "pick up the cube"


@dataclass
class FakeFoxglove:
    """The parts of the Foxglove API that upload.py uses, as documented."""

    url: str = ""
    uploads: dict[str, bytes] = field(default_factory=dict)
    upload_requests: list[dict[str, Any]] = field(default_factory=list)
    recordings: dict[str, dict[str, Any]] = field(default_factory=dict)
    episodes: dict[str, dict[str, Any]] = field(default_factory=dict)
    datasets: dict[str, dict[str, Any]] = field(default_factory=dict)
    failed_keys: set[str] = field(default_factory=set)


def _handler(api: FakeFoxglove) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args: Any) -> None:
            pass

        def _reply(self, status: int, body: Any = None) -> None:
            data = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def _body(self) -> bytes:
            return self.rfile.read(int(self.headers.get("Content-Length") or 0))

        def _authorized(self) -> bool:
            if self.headers.get("Authorization") == f"Bearer {API_KEY}":
                return True
            self._reply(401, {"error": "unauthorized"})
            return False

        def do_PUT(self) -> None:
            key = unquote(self.path.removeprefix("/signed/"))
            api.uploads[key] = self._body()
            api.recordings[key] = {
                "id": f"rec_{len(api.recordings)}",
                "key": key,
                "importStatus": "importing",
            }
            self._reply(200)

        def do_GET(self) -> None:
            if not self._authorized():
                return
            url = urlparse(self.path)
            if url.path == "/v1/data/pending-imports":
                key = url.query.removeprefix("key=")
                failed = key in api.failed_keys
                self._reply(
                    200, [{"status": "error", "error": "bad file"}] if failed else []
                )
                return
            recording = api.recordings.get(
                unquote(url.path.removeprefix("/v1/recordings/"))
            )
            if recording is None:
                self._reply(404, {"error": "not found"})
                return
            reply = dict(recording)
            if recording["key"] not in api.failed_keys:
                recording["importStatus"] = "complete"
            self._reply(200, reply)

        def do_POST(self) -> None:
            if not self._authorized():
                return
            body = json.loads(self._body() or b"{}")
            if self.path == "/v1/data/upload":
                api.upload_requests.append(body)
                self._reply(
                    200, {"link": f"{api.url}/signed/{body['key']}", "requestId": "r"}
                )
            elif self.path == "/v1/episodes":
                assert len(body["episodes"]) <= upload.BATCH_SIZE
                reply = []
                for episode in body["episodes"]:
                    episode_id = "ep_" + "_".join(episode["recordings"])
                    reply.append(
                        {"id": episode_id, "created": episode_id not in api.episodes}
                    )
                    api.episodes.setdefault(episode_id, episode)
                self._reply(200, {"episodes": reply})
            elif self.path == "/v1/datasets":
                assert len(body["episodeIds"]) <= upload.BATCH_SIZE
                if any(
                    dataset["name"] == body["name"] for dataset in api.datasets.values()
                ):
                    self._reply(409, {"error": "name taken"})
                    return
                dataset_id = f"ds_{len(api.datasets)}"
                api.datasets[dataset_id] = {**body, "versions": []}
                self._reply(200, {"id": dataset_id})
            elif match := re.fullmatch(r"/v1/datasets/(\w+)/commit", self.path):
                dataset = api.datasets[match[1]]
                dataset["versions"].append(list(dataset["episodeIds"]))
                committed = {
                    "versionNumber": len(dataset["versions"]),
                    "episodeCount": len(dataset["episodeIds"]),
                }
                self._reply(200, {"committed": committed})

        def do_PATCH(self) -> None:
            if not self._authorized():
                return
            match = re.fullmatch(r"/v1/datasets/(\w+)/episodes", self.path)
            assert match is not None
            added = json.loads(self._body())["add"]
            assert len(added) <= upload.BATCH_SIZE
            api.datasets[match[1]]["episodeIds"] += added
            self._reply(200, {"added": len(added), "removed": 0, "alreadyPresent": 0})

    return Handler


@pytest.fixture
def api(monkeypatch: pytest.MonkeyPatch) -> Iterator[FakeFoxglove]:
    fake = FakeFoxglove()
    server = ThreadingHTTPServer(("127.0.0.1", 0), _handler(fake))
    fake.url = f"http://127.0.0.1:{server.server_address[1]}"
    threading.Thread(target=server.serve_forever, daemon=True).start()
    monkeypatch.setenv("FOXGLOVE_API_URL", fake.url)
    monkeypatch.setenv("FOXGLOVE_API_KEY", API_KEY)
    monkeypatch.setattr(upload, "POLL_INTERVAL_S", 0)
    yield fake
    server.shutdown()


@pytest.fixture
def dataset(tmp_path: Path) -> Path:
    root = tmp_path / "pick_place"
    lengths = [4, 3, 5]
    scalar = {"shape": [1], "names": None}
    info = {
        "codebase_version": "v2.1",
        "fps": 10,
        "data_path": "data/chunk-{episode_chunk:03d}/episode_{episode_index:06d}.parquet",
        "video_path": None,
        "features": {
            "observation.state": {
                "dtype": "float32",
                "shape": [2],
                "names": ["x", "y"],
            },
            "timestamp": {"dtype": "float32", **scalar},
            "frame_index": {"dtype": "int64", **scalar},
            "episode_index": {"dtype": "int64", **scalar},
            "index": {"dtype": "int64", **scalar},
            "task_index": {"dtype": "int64", **scalar},
        },
    }
    (root / "data" / "chunk-000").mkdir(parents=True)
    (root / "meta").mkdir()
    (root / "meta" / "info.json").write_text(json.dumps(info))
    (root / "meta" / "tasks.jsonl").write_text(
        json.dumps({"task_index": 0, "task": TASK}) + "\n"
    )
    (root / "meta" / "episodes.jsonl").write_text(
        "".join(
            json.dumps({"episode_index": index, "tasks": [TASK], "length": length})
            + "\n"
            for index, length in enumerate(lengths)
        )
    )
    for index, length in enumerate(lengths):
        frames = pa.table(
            {
                "observation.state": pa.array(
                    [[frame, frame] for frame in range(length)], pa.list_(pa.float32())
                ),
                "timestamp": pa.array(
                    [frame / 10 for frame in range(length)], pa.float32()
                ),
                "frame_index": list(range(length)),
                "episode_index": [index] * length,
                "index": list(range(length)),
                "task_index": [0] * length,
            }
        )
        pq.write_table(
            frames, root / "data" / "chunk-000" / f"episode_{index:06d}.parquet"
        )
    return root


@pytest.fixture
def run(monkeypatch: pytest.MonkeyPatch, dataset: Path) -> Callable[..., None]:
    def run(*args: str) -> None:
        argv = ["upload.py", "--input", str(dataset), "--project-id", "prj_1", *args]
        monkeypatch.setattr(sys, "argv", argv)
        upload.main()

    return run


def test_converts_the_dataset_and_commits_its_episodes_as_a_dataset(
    api: FakeFoxglove, run: Callable[..., None], tmp_path: Path
) -> None:
    run("--output", str(tmp_path / "mcap"))

    paths = sorted((tmp_path / "mcap").glob("episode_*.mcap"))
    assert [request["deviceName"] for request in api.upload_requests] == [
        "lerobot-pick_place"
    ] * 3
    assert list(api.uploads.values()) == [path.read_bytes() for path in paths]
    assert [
        (episode["metadata"]["episode_index"], episode["metadata"]["tasks"])
        for episode in api.episodes.values()
    ] == [(str(index), json.dumps([TASK])) for index in range(3)]
    (dataset,) = api.datasets.values()
    assert dataset["name"] == "pick_place"
    assert dataset["versions"] == [list(api.episodes)]


def test_uploads_only_the_selected_episodes(
    api: FakeFoxglove, run: Callable[..., None]
) -> None:
    run("--episodes", "0,2")

    assert [request["key"] for request in api.upload_requests] == [
        "lerobot-pick_place-episode_000000",
        "lerobot-pick_place-episode_000002",
    ]
    (dataset,) = api.datasets.values()
    assert len(dataset["episodeIds"]) == 2


def test_adds_the_episodes_that_do_not_fit_in_the_first_request(
    api: FakeFoxglove, run: Callable[..., None], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(upload, "BATCH_SIZE", 2)

    run()

    (dataset,) = api.datasets.values()
    assert dataset["versions"] == [list(api.episodes)]


def test_skips_files_it_already_uploaded(
    api: FakeFoxglove, run: Callable[..., None]
) -> None:
    run()
    run("--dataset-name", "pick_place again")

    assert len(api.upload_requests) == 3
    first, second = api.datasets.values()
    assert first["versions"] == second["versions"]


def test_stops_when_an_import_fails(
    api: FakeFoxglove, run: Callable[..., None]
) -> None:
    api.failed_keys.add("lerobot-pick_place-episode_000001")

    with pytest.raises(SystemExit, match="episode_000001 failed: bad file"):
        run()
    assert api.datasets == {}


def test_requires_an_api_key(
    run: Callable[..., None], monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.delenv("FOXGLOVE_API_KEY", raising=False)

    with pytest.raises(SystemExit, match="FOXGLOVE_API_KEY"):
        run()
