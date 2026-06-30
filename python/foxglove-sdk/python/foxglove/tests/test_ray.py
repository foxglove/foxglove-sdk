import importlib
import importlib.util
import pickle
import sys
import types

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


@requires_ray
def test_dataset_id_resolution_not_implemented() -> None:
    """Until foxglove-client ships a dataset API, resolving a dataset_id is unimplemented."""
    from foxglove.ray import FoxgloveDataset

    with pytest.raises(NotImplementedError, match="dataset_id resolution"):
        FoxgloveDataset(token="t", dataset_id="ds")


@requires_ray
def test_get_read_tasks_buckets_recordings(monkeypatch: pytest.MonkeyPatch) -> None:
    from foxglove.ray import FoxgloveDataset

    monkeypatch.setattr(
        FoxgloveDataset,
        "_resolve_recording_ids",
        lambda self: ["r1", "r2", "r3", "r4", "r5"],
    )
    ds = FoxgloveDataset(token="t", dataset_id="ds")

    # parallelism caps the number of buckets; each non-empty bucket is one ReadTask.
    assert len(ds.get_read_tasks(parallelism=2)) == 2
    assert len(ds.get_read_tasks(parallelism=10)) == 5  # capped at recording count
    assert len(ds.get_read_tasks(parallelism=1)) == 1

    # The planned row count across all tasks equals the number of recordings.
    total_rows = sum(
        task.metadata.num_rows for task in ds.get_read_tasks(parallelism=2)
    )
    assert total_rows == 5


@requires_ray
def test_read_tasks_emit_one_mcap_block_per_recording(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    from foxglove.ray import FoxgloveDataset

    downloaded: list[str] = []

    class FakeClient:
        def __init__(self, *, token: str, host: str) -> None:
            self.token = token
            self.host = host

        def download_recording_data(self, *, id: str) -> bytes:
            downloaded.append(id)
            return b"MCAP-" + id.encode()

    # Inject a fake `foxglove.client` so the worker read fn can build a Client without the
    # real foxglove-client package or any network access.
    fake_client_mod = types.ModuleType("foxglove.client")
    fake_client_mod.Client = FakeClient  # type: ignore[attr-defined]
    monkeypatch.setitem(sys.modules, "foxglove.client", fake_client_mod)

    monkeypatch.setattr(
        FoxgloveDataset,
        "_resolve_recording_ids",
        lambda self: ["r1", "r2", "r3"],
    )
    ds = FoxgloveDataset(token="tok", dataset_id="ds", host="example.foxglove.dev")

    tasks = ds.get_read_tasks(parallelism=2)
    blocks = [block for task in tasks for block in task()]

    # One block (one row) per recording.
    assert len(blocks) == 3
    assert sorted(downloaded) == ["r1", "r2", "r3"]
    for block in blocks:
        assert len(block) == 1
        record = block["recording"].iloc[0]
        assert record["mcap"] == b"MCAP-" + record["id"].encode()


@requires_ray
def test_datasource_and_read_tasks_are_pickleable(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """ReadTasks are shipped to remote workers, so neither they nor the datasource may
    capture a live client."""
    from foxglove.ray import FoxgloveDataset

    monkeypatch.setattr(
        FoxgloveDataset, "_resolve_recording_ids", lambda self: ["r1", "r2"]
    )
    ds = FoxgloveDataset(token="t", dataset_id="ds")

    # No client object is stored on the instance -- only plain config.
    assert not any(type(v).__name__ == "Client" for v in ds.__dict__.values())

    # The datasource holds only plain config, so even stdlib pickle (no live client/socket) works.
    pickle.loads(pickle.dumps(ds))

    # Ray ships ReadTasks to workers with cloudpickle, which handles the read-fn closures.
    from ray import cloudpickle

    cloudpickle.loads(cloudpickle.dumps(ds.get_read_tasks(parallelism=2)))
