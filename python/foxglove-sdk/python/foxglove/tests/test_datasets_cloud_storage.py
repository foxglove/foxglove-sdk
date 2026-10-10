import builtins
import io
import sys
from pathlib import Path
from types import ModuleType
from typing import Any, cast
from unittest.mock import MagicMock, call

import pytest
from foxglove.datasets.storage import (
    ObjectLocation,
    _CloudObjectStore,
    _validate_location,
)


@pytest.fixture
def cloud_fs(monkeypatch: pytest.MonkeyPatch) -> MagicMock:
    fs = MagicMock()
    pyarrow = ModuleType("pyarrow")
    setattr(pyarrow, "fs", fs)
    monkeypatch.setitem(sys.modules, "pyarrow", pyarrow)
    return fs


def test_cloud_store_reuses_clients_across_providers_regions_and_accounts(
    cloud_fs: MagicMock,
) -> None:
    cloud_fs.resolve_s3_region.side_effect = {
        "east-a": "us-east-1",
        "east-b": "us-east-1",
        "west": "us-west-2",
    }.__getitem__
    east, west, gcs, azure_a, azure_b = [MagicMock() for _ in range(5)]
    cloud_fs.S3FileSystem.side_effect = [east, west]
    cloud_fs.GcsFileSystem.return_value = gcs
    cloud_fs.AzureFileSystem.side_effect = [azure_a, azure_b]
    locations = [
        (ObjectLocation("east-a", "prefix/first.mcap", scheme="s3"), east),
        (ObjectLocation("gcs-a", "first.mcap", scheme="gs"), gcs),
        (ObjectLocation("container", "first.mcap", "account-a", "az"), azure_a),
        (ObjectLocation("east-b", "second.mcap", scheme="s3"), east),
        (ObjectLocation("west", "first.mcap", scheme="s3"), west),
        (ObjectLocation("container", "second.mcap", "account-b", "az"), azure_b),
        (ObjectLocation("east-a", "third.mcap", scheme="s3"), east),
        (ObjectLocation("gcs-b", "second.mcap", scheme="gs"), gcs),
        (ObjectLocation("other", "second.mcap", "account-a", "az"), azure_a),
    ]
    store = _CloudObjectStore()
    for location, filesystem in locations:
        file = io.BytesIO(b"mcap")
        filesystem.open_input_file.return_value = file
        assert store.open(location) is file
        filesystem.open_input_file.assert_called_with(
            f"{location.bucket}/{location.path}"
        )

    # Only routing arguments are supplied, leaving native credential discovery intact.
    assert cloud_fs.resolve_s3_region.call_args_list == [
        call("east-a"),
        call("east-b"),
        call("west"),
    ]
    assert cloud_fs.S3FileSystem.call_args_list == [
        call(region="us-east-1"),
        call(region="us-west-2"),
    ]
    cloud_fs.GcsFileSystem.assert_called_once_with()
    assert cloud_fs.AzureFileSystem.call_args_list == [
        call(account_name="account-a"),
        call(account_name="account-b"),
    ]


def test_repeated_opens_return_fresh_files(cloud_fs: MagicMock) -> None:
    filesystem = cloud_fs.GcsFileSystem.return_value
    first, second = io.BytesIO(b"first"), io.BytesIO(b"second")
    filesystem.open_input_file.side_effect = [first, second]
    store = _CloudObjectStore()
    location = ObjectLocation("bucket", "file.mcap", scheme="gs")

    with store.open(location) as file:
        assert file.read() == b"first"
    with store.open(location) as file:
        assert file.read() == b"second"

    assert first.closed and second.closed
    cloud_fs.GcsFileSystem.assert_called_once_with()
    assert filesystem.open_input_file.call_args_list == [
        call("bucket/file.mcap"),
        call("bucket/file.mcap"),
    ]


@pytest.mark.parametrize("scheme", [None, "", "https", "s3a", "file"])
def test_missing_or_unsupported_scheme_fails_before_import(
    scheme: str | None, monkeypatch: pytest.MonkeyPatch
) -> None:
    imports: list[str] = []
    original_import = builtins.__import__

    def import_module(name: str, *args: Any, **kwargs: Any) -> Any:
        if name.startswith("pyarrow"):
            imports.append(name)
            raise ImportError("PyArrow is unavailable")
        return original_import(name, *args, **kwargs)

    monkeypatch.setattr(builtins, "__import__", import_module)
    store = _CloudObjectStore()
    location = ObjectLocation("bucket", "file.mcap", scheme=scheme)
    for validate in (_validate_location, store.open):
        with pytest.raises(ValueError, match="object_store_factory"):
            validate(location)
    assert imports == []


@pytest.mark.parametrize("account", [None, ""])
def test_azure_requires_account_before_creating_filesystem(
    account: str | None, cloud_fs: MagicMock
) -> None:
    location = ObjectLocation("container", "file.mcap", account, "az")
    for validate in (_validate_location, _CloudObjectStore().open):
        with pytest.raises(ValueError, match="azure_storage_account_name"):
            validate(location)
    assert cloud_fs.mock_calls == []


@pytest.mark.parametrize("scheme", ["s3", "gs", "az"])
def test_cloud_open_errors_propagate_without_fallback(
    scheme: str, cloud_fs: MagicMock
) -> None:
    cloud_fs.resolve_s3_region.return_value = "us-east-1"
    for constructor in (
        cloud_fs.S3FileSystem,
        cloud_fs.GcsFileSystem,
        cloud_fs.AzureFileSystem,
    ):
        constructor.return_value.open_input_file.side_effect = PermissionError(
            "Access denied"
        )
    with pytest.raises(PermissionError, match="Access denied"):
        _CloudObjectStore().open(ObjectLocation("bucket", "file.mcap", "a", scheme))


def test_s3_region_discovery_errors_propagate(cloud_fs: MagicMock) -> None:
    cloud_fs.resolve_s3_region.side_effect = OSError("Region unavailable")
    with pytest.raises(OSError, match="Region unavailable"):
        _CloudObjectStore().open(ObjectLocation("bucket", "file.mcap", scheme="s3"))
    cloud_fs.S3FileSystem.assert_not_called()


def test_cloud_store_handles_are_seekable_and_bufferable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    fs = pytest.importorskip("pyarrow.fs")
    local = fs.LocalFileSystem()
    monkeypatch.setattr(fs, "GcsFileSystem", lambda: local)
    recording = tmp_path / "recording.mcap"
    recording.write_bytes(b"recording contents")

    with _CloudObjectStore().open(
        ObjectLocation(str(tmp_path), "recording.mcap", scheme="gs")
    ) as file:
        with io.BufferedReader(
            cast(io.RawIOBase, file), buffer_size=64 * 1024
        ) as buffered:
            assert buffered.seekable()
            assert buffered.read(9) == b"recording"
            buffered.seek(10)
            assert buffered.read() == b"contents"
