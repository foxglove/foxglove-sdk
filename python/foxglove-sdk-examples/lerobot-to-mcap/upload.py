"""Convert a LeRobot dataset into MCAP files, upload them to Foxglove, make a Foxglove
episode of each, and commit them all as a new dataset.

Set FOXGLOVE_API_KEY to an API key that can upload data and manage episodes and datasets,
and FOXGLOVE_API_URL to use a Foxglove API other than https://api.foxglove.dev.
"""

import argparse
import hashlib
import json
import math
import os
import sys
import tempfile
import time
from collections.abc import Iterator
from pathlib import Path
from typing import Any, TypeVar

import requests
from foxglove.lerobot import DatasetMetadata
from main import add_conversion_arguments, convert
from mcap.reader import make_reader

DEFAULT_API_URL = "https://api.foxglove.dev"
BATCH_SIZE = 2000
PAGE_SIZE = 2000
POLL_INTERVAL_S = 10

T = TypeVar("T")


class Api:
    def __init__(self, url: str, key: str) -> None:
        self.url = url.rstrip("/")
        self.session = requests.Session()
        self.session.headers["Authorization"] = f"Bearer {key}"

    def call(
        self, method: str, path: str, *, hint: str | None = None, **kwargs: Any
    ) -> Any:
        response = self.session.request(method, self.url + path, timeout=60, **kwargs)
        if not response.ok:
            message = f"error: {method} {path}: {response.status_code} {response.text}"
            sys.exit(message if hint is None else f"{message}\n{hint}")
        return response.json()

    def list_all(self, path: str, params: dict[str, str]) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []
        while True:
            page = self.call(
                "GET", path, params={**params, "limit": PAGE_SIZE, "offset": len(items)}
            )
            items += page
            if len(page) < PAGE_SIZE:
                return items


def batches(items: list[T]) -> Iterator[list[T]]:
    for start in range(0, len(items), BATCH_SIZE):
        yield items[start : start + BATCH_SIZE]


def content_key(path: Path, device_name: str, project_id: str) -> str:
    digest = hashlib.sha256(project_id.encode())
    with path.open("rb") as file:
        reader = make_reader(file)
        for record in reader.iter_metadata():
            digest.update(
                json.dumps([record.name, sorted(record.metadata.items())]).encode()
            )
        for attachment in reader.iter_attachments():
            digest.update(json.dumps([attachment.name, attachment.log_time]).encode())
            digest.update(attachment.data)
        for schema, channel, message in reader.iter_messages():
            digest.update(
                json.dumps(
                    [
                        channel.topic,
                        schema.name if schema else "",
                        message.log_time,
                        message.publish_time,
                    ]
                ).encode()
            )
            digest.update(message.data)
    return f"{device_name}-{path.stem}-{digest.hexdigest()[:16]}"


def dataset_name(args: argparse.Namespace) -> str:
    return str(args.dataset_name or args.input.resolve().name)


def check_dataset_name(api: Api, project_id: str, name: str) -> None:
    datasets = api.list_all("/datasets", {"projectId": project_id, "name": name})
    taken = [d["name"] for d in datasets if d["name"].casefold() == name.casefold()]
    if taken:
        sys.exit(
            f"error: project {project_id} already has a dataset named {taken[0]!r}. "
            "Choose another name with --dataset-name, or delete that dataset."
        )


def lerobot_metadata(path: Path) -> dict[str, str]:
    with path.open("rb") as file:
        for record in make_reader(file).iter_metadata():
            if record.name == "lerobot":
                return dict(record.metadata)
    return {}


def upload(api: Api, path: Path, key: str, device_name: str, project_id: str) -> str:
    response = api.call(
        "POST",
        "/data/upload",
        json={
            "filename": path.name,
            "key": key,
            "deviceName": device_name,
            "projectId": project_id,
        },
    )
    with path.open("rb") as body:
        uploaded = requests.put(
            response["link"],
            data=body,
            headers={"Content-Type": "application/octet-stream"},
            timeout=600,
        )
    if not uploaded.ok:
        sys.exit(
            f"error: uploading {path.name}: {uploaded.status_code} {uploaded.text}"
        )
    print(f"{path.name}: uploaded")
    request_id: str = response["requestId"]
    return request_id


def wait_for_imports(
    api: Api,
    filenames: dict[str, str],
    request_ids: dict[str, str],
    filters: dict[str, str],
    timeout_s: float,
) -> dict[str, str]:
    deadline = time.monotonic() + timeout_s
    while True:
        recordings = {
            recording["key"]: recording
            for recording in api.list_all("/recordings", filters)
            if recording.get("key") in filenames
        }
        for key, recording in recordings.items():
            if recording["importStatus"] == "failed":
                sys.exit(f"error: importing {filenames[key]} failed")
        for item in api.list_all("/data/pending-imports", filters):
            if item.get("status") == "error" and item.get("requestId") in request_ids:
                sys.exit(
                    f"error: importing {request_ids[item['requestId']]} failed: "
                    f"{item.get('error', '')}"
                )
        recording_ids = {
            key: recording["id"]
            for key, recording in recordings.items()
            if recording["importStatus"] == "complete"
        }
        waiting = len(filenames) - len(recording_ids)
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
    name = dataset_name(args)
    device_name = args.device_name or f"lerobot-{source_name}"
    filters = {"deviceName": device_name, "projectId": args.project_id}
    keys = {path: content_key(path, device_name, args.project_id) for path in paths}

    uploaded = {
        recording.get("key") for recording in api.list_all("/recordings", filters)
    }
    request_ids: dict[str, str] = {}
    for path in paths:
        if keys[path] in uploaded:
            print(f"{path.name}: already uploaded")
            continue
        request_id = upload(api, path, keys[path], device_name, args.project_id)
        request_ids[request_id] = path.name
    recording_ids = wait_for_imports(
        api,
        {keys[path]: path.name for path in paths},
        request_ids,
        filters,
        args.import_timeout,
    )

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
            "name": name,
            "description": f"LeRobot dataset {source_name}",
            "episodeIds": first,
        },
        hint=f"If a dataset named {name!r} already exists in project "
        f"{args.project_id}, choose another name with --dataset-name, or delete it.",
    )
    uncommitted = (
        f"Dataset {dataset['id']} ({name!r}) was created but not committed, and it "
        "keeps the name: delete it before trying again, or choose another name with "
        "--dataset-name."
    )
    for more in rest:
        api.call(
            "PATCH",
            f"/datasets/{dataset['id']}/episodes",
            json={"add": more},
            hint=uncommitted,
        )
    version = api.call("POST", f"/datasets/{dataset['id']}/commit", hint=uncommitted)[
        "committed"
    ]
    print(
        f"committed version {version['versionNumber']} of dataset {name!r} "
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
    if not math.isfinite(args.import_timeout) or args.import_timeout < 0:
        parser.error("--import-timeout must be a finite number of seconds, 0 or more")

    api_key = os.environ.get("FOXGLOVE_API_KEY")
    if not api_key:
        sys.exit("error: set FOXGLOVE_API_KEY to a Foxglove API key")
    api_url = os.environ.get("FOXGLOVE_API_URL") or DEFAULT_API_URL
    api = Api(api_url.rstrip("/") + "/v1", api_key)
    check_dataset_name(api, args.project_id, dataset_name(args))

    if args.output is not None:
        upload_dataset(args, api, *convert(args, args.output))
        return
    with tempfile.TemporaryDirectory() as output:
        upload_dataset(args, api, *convert(args, Path(output)))


if __name__ == "__main__":
    main()
