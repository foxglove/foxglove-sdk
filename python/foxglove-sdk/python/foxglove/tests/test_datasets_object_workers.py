import os
import threading
from collections.abc import Iterator
from datetime import timedelta
from functools import partial
from pathlib import Path
from typing import IO, Any

import pytest
from foxglove.datasets import EpisodeReader, ObjectLocation

from .test_datasets import Client
from .test_datasets_storage import START, START_NS, mcap_bytes

os.environ.setdefault("RAY_ENABLE_UV_RUN_RUNTIME_ENV", "0")


class ObjectPage:
    def __init__(self, directory: str) -> None:
        self.directory = directory

    def auto_paging_iter(self) -> Iterator[dict[str, Any]]:
        for index in range(4):
            yield {
                "episode": {
                    "id": str(index),
                    "start_time": START + timedelta(seconds=index),
                    "end_time": START + timedelta(seconds=index, microseconds=1),
                    "metadata": {},
                    "recordings": [
                        {
                            "id": "recording",
                            "available": True,
                            "location": {"bucket": self.directory, "path": "data.mcap"},
                        }
                    ],
                }
            }


class ObjectClient(Client):
    def __init__(self, directory: str, driver_pid: int) -> None:
        assert os.getpid() == driver_pid, "API clients must stay on the driver"
        super().__init__()
        self.directory = directory

    def get_dataset_version_episodes(
        self,
        *,
        dataset_id: str,
        version_number: int,
        limit: int,
        include_recordings: bool = False,
    ) -> Any:
        assert include_recordings
        return ObjectPage(self.directory)


class ArrowStore:
    def __init__(self, driver_pid: int) -> None:
        import pyarrow.fs as fs

        assert os.getpid() != driver_pid, "Storage must be created in workers"
        self.filesystem = fs.LocalFileSystem()

    def open(self, location: ObjectLocation) -> IO[bytes]:
        return self.filesystem.open_input_file(  # type: ignore[no-any-return]
            f"{location.bucket}/{location.path}"
        )

    def __getstate__(self) -> Any:
        raise AssertionError("Storage clients must never be serialized")


def object_samples(episode: EpisodeReader) -> Iterator[dict[str, Any]]:
    for _, channel, message, decoded in episode.iter_messages():
        yield {
            "episode": episode.id,
            "topic": channel.topic,
            "time": message.log_time,
            "value": decoded,
        }


@pytest.mark.parametrize("framework", ["torch", "ray"])
def test_direct_storage_in_real_workers(tmp_path: Path, framework: str) -> None:
    pytest.importorskip(framework)
    pytest.importorskip("pyarrow")
    (tmp_path / "data.mcap").write_bytes(
        mcap_bytes(
            [
                (topic, index * 1_000_000_000 + offset, index * 10 + offset)
                for index in range(4)
                for offset in (0, 1000, 1001)
                for topic in ("/selected", "/other")
            ]
        )
    )
    driver_lock = threading.Lock()

    def make_client() -> ObjectClient:
        # This planning-only factory cannot be pickled or sent to Ray workers.
        with driver_lock:
            return ObjectClient(str(tmp_path), os.getpid())

    options: dict[str, Any] = {
        "version": 7,
        "topics": ["/selected"],
        "client_factory": make_client,
        "object_store_factory": partial(ArrowStore, os.getpid()),
        "read_episode": object_samples,
    }
    expected = [
        {
            "episode": str(index),
            "topic": "/selected",
            "time": START_NS + index * 1_000_000_000 + offset,
            "value": index * 10 + offset,
        }
        for index in range(4)
        for offset in (0, 1000)
    ]
    rows = []
    if framework == "torch":
        from foxglove.datasets.torch import read_dataset
        from torch.utils.data import DataLoader

        for rank in range(2):
            dataset = read_dataset("dataset", rank=rank, world_size=2, **options)
            loader = DataLoader(
                dataset,
                batch_size=None,
                num_workers=2,
                multiprocessing_context="spawn",
                persistent_workers=True,
            )
            first_epoch = list(loader)
            assert list(loader) == first_epoch
            rows.extend(first_epoch)
            del loader
    else:
        import ray
        from foxglove.datasets.ray import read_dataset as read_ray_dataset

        ray.init(num_cpus=2, include_dashboard=False)
        try:
            ray_dataset = read_ray_dataset("dataset", concurrency=2, **options)
            rows = ray_dataset.take_all()
        finally:
            ray.shutdown()
    assert sorted(rows, key=lambda row: (row["episode"], row["time"])) == expected
