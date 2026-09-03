import importlib
import importlib.util
import pickle
import sys
from typing import Any
from unittest.mock import Mock

import pytest

ray_installed = importlib.util.find_spec("ray") is not None
requires_ray = pytest.mark.skipif(
    not ray_installed, reason="the `ray` extra is not installed"
)


def test_missing_extra_raises_friendly_error(monkeypatch: pytest.MonkeyPatch) -> None:
    """Importing foxglove.ray without the extra installed raises a helpful message."""
    # Force a fresh import of the subpackage, with `ray` made unimportable so the gated
    # import in foxglove/ray/__init__.py takes its `except ImportError` branch.
    for name in list(sys.modules):
        if name == "ray" or name.startswith("ray.") or name.startswith("foxglove.ray"):
            monkeypatch.delitem(sys.modules, name, raising=False)
    # A `None` entry in sys.modules makes `import ray` raise ImportError.
    monkeypatch.setitem(sys.modules, "ray", None)

    with pytest.raises(ImportError, match=r"foxglove-sdk\[ray\]"):
        importlib.import_module("foxglove.ray")


def _json_response(payload: dict[str, Any]) -> Mock:
    resp = Mock()
    resp.json.return_value = payload
    resp.content = b""
    resp.raise_for_status.return_value = None
    return resp


def _bytes_response(content: bytes) -> Mock:
    resp = Mock()
    resp.content = content
    resp.raise_for_status.return_value = None
    return resp


def _episode_entry(episode_id: str) -> dict[str, Any]:
    return {"episode": {"id": episode_id}}


@requires_ray
def test_v1_url_accepts_host_or_full_url() -> None:
    from foxglove.ray.datasource import FoxgloveClient

    cases = [
        ("api.foxglove.dev", "https://api.foxglove.dev/v1/datasets/ds/episodes"),
        (
            "https://api.foxglove.dev",
            "https://api.foxglove.dev/v1/datasets/ds/episodes",
        ),
        (
            "https://api.foxglove.dev/extra/path",
            "https://api.foxglove.dev/v1/datasets/ds/episodes",
        ),
        ("http://localhost:8080", "http://localhost:8080/v1/datasets/ds/episodes"),
    ]
    for base_url, expected in cases:
        client = FoxgloveClient(base_url, "key", "ds")
        assert client._v1_url("/datasets/ds/episodes") == expected


@requires_ray
def test_list_episodes_pages_until_short_batch(monkeypatch: pytest.MonkeyPatch) -> None:
    from foxglove.ray.datasource import _EPISODES_PAGE, FoxgloveClient

    page1 = [_episode_entry(f"ep{i}") for i in range(_EPISODES_PAGE)]
    page2 = [_episode_entry("ep-last")]
    calls: list[dict[str, Any]] = []

    def fake_get(
        url: str, headers: dict[str, str], params: dict[str, int], timeout: int
    ) -> Mock:
        calls.append(
            {"url": url, "headers": headers, "params": params, "timeout": timeout}
        )
        batch = page1 if params["offset"] == 0 else page2
        return _json_response({"episodes": batch})

    monkeypatch.setattr("foxglove.ray.datasource.requests.get", fake_get)

    client = FoxgloveClient("https://api.foxglove.dev", "tok", "ds_1")
    episodes = client.list_episodes()

    assert [ep["id"] for ep in episodes] == [
        f"ep{i}" for i in range(_EPISODES_PAGE)
    ] + ["ep-last"]
    assert len(calls) == 2
    assert calls[0]["url"].endswith("/v1/datasets/ds_1/episodes")
    assert calls[0]["params"] == {"limit": _EPISODES_PAGE, "offset": 0}
    assert calls[1]["params"] == {"limit": _EPISODES_PAGE, "offset": _EPISODES_PAGE}
    assert calls[0]["headers"]["Authorization"] == "Bearer tok"


@requires_ray
def test_fetch_episode_streams_signed_link(monkeypatch: pytest.MonkeyPatch) -> None:
    from foxglove.ray.datasource import FoxgloveClient

    posted: list[dict[str, Any]] = []
    gotten: list[dict[str, Any]] = []

    def fake_post(
        url: str, headers: dict[str, str], json: dict[str, str], timeout: int
    ) -> Mock:
        posted.append(
            {"url": url, "headers": headers, "json": json, "timeout": timeout}
        )
        return _json_response({"link": "https://signed.example/mcap"})

    def fake_get(url: str, stream: bool = False, timeout: int = 0) -> Mock:
        gotten.append({"url": url, "stream": stream, "timeout": timeout})
        return _bytes_response(b"mcap-bytes")

    monkeypatch.setattr("foxglove.ray.datasource.requests.post", fake_post)
    monkeypatch.setattr("foxglove.ray.datasource.requests.get", fake_get)

    client = FoxgloveClient("api.foxglove.dev", "tok", "ds_1")
    record = client.fetch_episode("ep-9")

    assert record == {"episode_id": "ep-9", "mcap": b"mcap-bytes"}
    assert posted[0]["url"] == "https://api.foxglove.dev/v1/data/stream"
    assert posted[0]["json"] == {"episodeId": "ep-9"}
    assert gotten[0] == {
        "url": "https://signed.example/mcap",
        "stream": True,
        "timeout": 600,
    }


@requires_ray
def test_empty_dataset_yields_no_read_tasks(monkeypatch: pytest.MonkeyPatch) -> None:
    from foxglove.ray import FoxgloveDataset
    from foxglove.ray.datasource import FoxgloveClient

    monkeypatch.setattr(FoxgloveClient, "list_episodes", lambda self: [])
    ds = FoxgloveDataset(api_key="t", dataset_id="ds")
    assert ds.get_read_tasks(parallelism=4) == []


@requires_ray
def test_get_read_tasks_buckets_episodes(monkeypatch: pytest.MonkeyPatch) -> None:
    from foxglove.ray import FoxgloveDataset
    from foxglove.ray.datasource import FoxgloveClient

    monkeypatch.setattr(
        FoxgloveClient,
        "list_episodes",
        lambda self: [{"id": f"e{i}"} for i in range(1, 6)],
    )
    ds = FoxgloveDataset(api_key="t", dataset_id="ds")

    # parallelism caps the number of buckets; each non-empty bucket is one ReadTask.
    assert len(ds.get_read_tasks(parallelism=2)) == 2
    assert len(ds.get_read_tasks(parallelism=10)) == 5  # capped at episode count
    assert len(ds.get_read_tasks(parallelism=1)) == 1

    # The planned row count across all tasks equals the number of episodes.
    total_rows = sum(
        task.metadata.num_rows for task in ds.get_read_tasks(parallelism=2)
    )
    assert total_rows == 5


@requires_ray
def test_per_task_row_limit_truncates_each_bucket(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from foxglove.ray import FoxgloveDataset
    from foxglove.ray.datasource import FoxgloveClient

    monkeypatch.setattr(
        FoxgloveClient,
        "list_episodes",
        lambda self: [{"id": f"e{i}"} for i in range(1, 6)],
    )
    ds = FoxgloveDataset(api_key="t", dataset_id="ds")
    tasks = ds.get_read_tasks(parallelism=2, per_task_row_limit=1)
    assert len(tasks) == 2
    assert all(task.metadata.num_rows == 1 for task in tasks)


@requires_ray
def test_read_tasks_emit_one_mcap_block_per_episode(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from foxglove.ray import FoxgloveDataset
    from foxglove.ray.datasource import FoxgloveClient

    downloaded: list[str] = []

    monkeypatch.setattr(
        FoxgloveClient,
        "list_episodes",
        lambda self: [{"id": "e1"}, {"id": "e2"}, {"id": "e3"}],
    )

    def fake_fetch(self: object, episode_id: str) -> dict[str, Any]:
        downloaded.append(episode_id)
        return {"episode_id": episode_id, "mcap": b"MCAP-" + episode_id.encode()}

    monkeypatch.setattr(FoxgloveClient, "fetch_episode", fake_fetch)

    ds = FoxgloveDataset(
        api_key="tok", dataset_id="ds", base_url="https://example.foxglove.dev"
    )

    tasks = ds.get_read_tasks(parallelism=2)
    blocks = [block for task in tasks for block in task()]

    # One block (one row) per episode.
    assert len(blocks) == 3
    assert sorted(downloaded) == ["e1", "e2", "e3"]
    for block in blocks:
        assert len(block) == 1
        record = block["episode"].iloc[0]
        assert record["mcap"] == b"MCAP-" + record["episode_id"].encode()


@requires_ray
def test_datasource_and_read_tasks_are_pickleable(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """ReadTasks are shipped to remote workers, so neither they nor the datasource may
    capture a live client."""
    from foxglove.ray import FoxgloveDataset
    from foxglove.ray.datasource import FoxgloveClient

    monkeypatch.setattr(
        FoxgloveClient, "list_episodes", lambda self: [{"id": "e1"}, {"id": "e2"}]
    )
    ds = FoxgloveDataset(api_key="t", dataset_id="ds")

    # No client object is stored on the instance -- only plain config.
    assert not any(type(v).__name__ == "FoxgloveClient" for v in ds.__dict__.values())

    # The datasource holds only plain config, so even stdlib pickle works.
    pickle.loads(pickle.dumps(ds))

    # Ray ships ReadTasks to workers with cloudpickle, which handles the read-fn closures.
    from ray import cloudpickle

    cloudpickle.loads(cloudpickle.dumps(ds.get_read_tasks(parallelism=2)))
