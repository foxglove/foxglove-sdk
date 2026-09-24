"""Convert a LeRobot dataset into MCAP files, upload them to Foxglove, make a Foxglove
episode of each, and commit them all as a new dataset.

Set FOXGLOVE_API_KEY to an API key that can upload data and manage episodes and datasets,
and FOXGLOVE_API_URL to use a Foxglove API other than https://api.foxglove.dev.
"""

import argparse
import os
import sys
import tempfile
import time
from collections.abc import Iterator
from pathlib import Path
from typing import Any, TypeVar
from urllib.parse import quote

import requests
from foxglove.lerobot import DatasetMetadata
from main import add_conversion_arguments, convert
from mcap.reader import make_reader

DEFAULT_API_URL = "https://api.foxglove.dev"
BATCH_SIZE = 2000
POLL_INTERVAL_S = 10

T = TypeVar("T")


class Api:
    def __init__(self, url: str, key: str) -> None:
        self.url = url.rstrip("/")
        self.session = requests.Session()
        self.session.headers["Authorization"] = f"Bearer {key}"

    def call(self, method: str, path: str, **kwargs: Any) -> Any:
        response = self.session.request(method, self.url + path, timeout=60, **kwargs)
        if not response.ok:
            sys.exit(f"error: {method} {path}: {response.status_code} {response.text}")
        return response.json()

    def recording(self, key: str) -> dict[str, Any] | None:
        response = self.session.get(
            f"{self.url}/recordings/{quote(key, safe='')}", timeout=60
        )
        if response.status_code == 404:
            return None
        if not response.ok:
            sys.exit(
                f"error: GET recording {key}: {response.status_code} {response.text}"
            )
        recording: dict[str, Any] = response.json()
        return recording


def batches(items: list[T]) -> Iterator[list[T]]:
    for start in range(0, len(items), BATCH_SIZE):
        yield items[start : start + BATCH_SIZE]


def lerobot_metadata(path: Path) -> dict[str, str]:
    with path.open("rb") as file:
        for record in make_reader(file).iter_metadata():
            if record.name == "lerobot":
                return dict(record.metadata)
    return {}


def upload(api: Api, path: Path, key: str, device_name: str, project_id: str) -> None:
    if api.recording(key) is not None:
        print(f"{path.name}: already uploaded")
        return
    link = api.call(
        "POST",
        "/data/upload",
        json={
            "filename": path.name,
            "key": key,
            "deviceName": device_name,
            "projectId": project_id,
        },
    )["link"]
    with path.open("rb") as body:
        response = requests.put(
            link,
            data=body,
            headers={"Content-Type": "application/octet-stream"},
            timeout=600,
        )
    if not response.ok:
        sys.exit(
            f"error: uploading {path.name}: {response.status_code} {response.text}"
        )
    print(f"{path.name}: uploaded")


def wait_for_imports(api: Api, keys: list[str], timeout_s: float) -> dict[str, str]:
    recording_ids: dict[str, str] = {}
    deadline = time.monotonic() + timeout_s
    while True:
        for key in keys:
            if key in recording_ids:
                continue
            recording = api.recording(key)
            if recording is not None and recording["importStatus"] == "complete":
                recording_ids[key] = recording["id"]
                continue
            imports = api.call("GET", "/data/pending-imports", params={"key": key})
            errors = [
                item.get("error") for item in imports if item.get("status") == "error"
            ]
            if errors or (
                recording is not None and recording["importStatus"] == "failed"
            ):
                sys.exit(
                    f"error: importing {key} failed: {errors[0] if errors else ''}"
                )
        waiting = len(keys) - len(recording_ids)
        if not waiting:
            return recording_ids
        if time.monotonic() > deadline:
            sys.exit(
                f"error: {waiting} recording(s) still importing after {timeout_s:.0f}s"
            )
        print(f"waiting for {waiting} recording(s) to import")
        time.sleep(POLL_INTERVAL_S)


def upload_dataset(
    args: argparse.Namespace, api: Api, metadata: DatasetMetadata, paths: list[Path]
) -> None:
    source_name = metadata.root.name
    dataset_name = args.dataset_name or source_name
    device_name = args.device_name or f"lerobot-{source_name}"
    keys = {path: f"{device_name}-{path.stem}" for path in paths}

    for path in paths:
        upload(api, path, keys[path], device_name, args.project_id)
    recording_ids = wait_for_imports(api, list(keys.values()), args.import_timeout)

    episode_ids: list[str] = []
    for batch in batches(paths):
        episodes = api.call(
            "POST",
            "/episodes",
            json={
                "projectId": args.project_id,
                "episodes": [
                    {
                        "recordings": [recording_ids[keys[path]]],
                        "metadata": lerobot_metadata(path),
                    }
                    for path in batch
                ],
            },
        )["episodes"]
        episode_ids += [episode["id"] for episode in episodes]
    print(f"made {len(episode_ids)} episode(s)")

    first, *rest = list(batches(episode_ids))
    dataset = api.call(
        "POST",
        "/datasets",
        json={
            "projectId": args.project_id,
            "name": dataset_name,
            "description": f"LeRobot dataset {source_name}",
            "episodeIds": first,
        },
    )
    for more in rest:
        api.call("PATCH", f"/datasets/{dataset['id']}/episodes", json={"add": more})
    version = api.call("POST", f"/datasets/{dataset['id']}/commit")["committed"]
    print(
        f"committed version {version['versionNumber']} of dataset {dataset_name!r} "
        f"({dataset['id']}) with {version['episodeCount']} episode(s)"
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__.split("\n\n")[0].replace("\n", " ")
    )
    add_conversion_arguments(parser)
    parser.add_argument(
        "--output",
        type=Path,
        help="directory to keep the MCAP files in (default: a temporary directory, "
        "deleted afterwards)",
    )
    parser.add_argument(
        "--project-id", required=True, help="Foxglove project to add the dataset to"
    )
    parser.add_argument(
        "--dataset-name",
        help="name of the new dataset (default: the LeRobot dataset's name)",
    )
    parser.add_argument(
        "--device-name",
        help="device to upload the recordings to (default: lerobot-<dataset name>)",
    )
    parser.add_argument(
        "--import-timeout",
        type=float,
        default=3600,
        help="seconds to wait for uploads to import (default: %(default)s)",
    )
    args = parser.parse_args()

    api_key = os.environ.get("FOXGLOVE_API_KEY")
    if not api_key:
        sys.exit("error: set FOXGLOVE_API_KEY to a Foxglove API key")
    api_url = os.environ.get("FOXGLOVE_API_URL") or DEFAULT_API_URL
    api = Api(api_url.rstrip("/") + "/v1", api_key)

    if args.output is not None:
        upload_dataset(args, api, *convert(args, args.output))
        return
    with tempfile.TemporaryDirectory() as output:
        upload_dataset(args, api, *convert(args, Path(output)))


if __name__ == "__main__":
    main()
