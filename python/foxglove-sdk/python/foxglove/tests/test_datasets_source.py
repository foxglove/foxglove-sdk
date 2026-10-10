import importlib
import pickle
from collections.abc import Iterator
from typing import Any
from unittest.mock import MagicMock, call

import pytest
from foxglove.datasets import EpisodeReader
from foxglove.datasets.reader import _make_client, _plan
from foxglove.datasets.storage import ObjectLocation

from .datasets_helpers import Client, Store, mcap_bytes, plan_client


def first_sample(episode: EpisodeReader) -> Iterator[Any]:
    yield next(episode.iter_messages())[3]


def test_default_api_client_reads_environment_again_after_plan_serialization(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    client_module = importlib.import_module("foxglove.client")
    constructor = MagicMock(side_effect=lambda **kwargs: Client())
    monkeypatch.setattr(client_module, "Client", constructor)
    monkeypatch.setenv("FOXGLOVE_API_TOKEN", "driver-token-must-not-be-serialized")
    plan = _plan("dataset", 7, ["/camera"])
    constructor.assert_called_once_with(token="driver-token-must-not-be-serialized")
    serialized = pickle.dumps(plan)
    assert b"driver-token-must-not-be-serialized" not in serialized
    assert plan.client_factory is _make_client
    worker_plan = pickle.loads(serialized)
    stream = worker_plan.read(worker_plan.episodes, first_sample)
    constructor.assert_called_once()

    monkeypatch.setenv("FOXGLOVE_API_TOKEN", "worker-token")
    assert list(stream) == ["a", "b", "c", "d"]
    monkeypatch.setenv("FOXGLOVE_API_TOKEN", "next-iteration-token")
    assert list(worker_plan.read(worker_plan.episodes, first_sample)) == [
        "a",
        "b",
        "c",
        "d",
    ]
    assert constructor.call_args_list == [
        call(token="driver-token-must-not-be-serialized"),
        call(token="worker-token"),
        call(token="next-iteration-token"),
    ]


@pytest.mark.parametrize("token", [None, ""])
def test_missing_default_api_token_is_actionable_before_network(
    token: str | None, monkeypatch: pytest.MonkeyPatch
) -> None:
    client_module = importlib.import_module("foxglove.client")
    constructor = MagicMock()
    monkeypatch.setattr(client_module, "Client", constructor)
    if token is None:
        monkeypatch.delenv("FOXGLOVE_API_TOKEN", raising=False)
    else:
        monkeypatch.setenv("FOXGLOVE_API_TOKEN", token)
    with pytest.raises(ValueError, match="FOXGLOVE_API_TOKEN.*client_factory"):
        _plan("dataset", 7, ["/camera"])
    constructor.assert_not_called()


def test_custom_api_factory_takes_precedence_without_default_token(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("FOXGLOVE_API_TOKEN", raising=False)
    client_module = importlib.import_module("foxglove.client")
    constructor = MagicMock()
    monkeypatch.setattr(client_module, "Client", constructor)
    factory = MagicMock(side_effect=Client)
    plan = _plan("dataset", 7, ["/camera"], factory)
    assert list(plan.read(plan.episodes, first_sample)) == ["a", "b", "c", "d"]
    assert factory.call_count == 2
    constructor.assert_not_called()


def recording(
    location: dict[str, Any] | None, *, available: bool = True
) -> dict[str, Any]:
    return {"id": "recording", "available": available, "location": location}


@pytest.mark.parametrize("scheme", [None, "", "https", "file"])
def test_builtin_object_storage_source_validates_scheme_before_constructing_storage(
    scheme: str | None, monkeypatch: pytest.MonkeyPatch
) -> None:
    client = plan_client(
        [recording({"bucket": "bucket", "path": "file.mcap", "scheme": scheme})]
    )
    store_factory = MagicMock()
    monkeypatch.setattr("foxglove.datasets.reader._CloudObjectStore", store_factory)
    with pytest.raises(ValueError, match="object_store_factory"):
        _plan("dataset", 7, ["/selected"], lambda: client, source="object_storage")
    store_factory.assert_not_called()
    client.get_dataset_version_episodes.assert_called_once_with(
        dataset_id="dataset", version_number=7, limit=2000, include_recordings=True
    )


@pytest.mark.parametrize("adapter", ["torch", "ray"])
def test_public_adapters_forward_object_storage_source_without_creating_storage(
    adapter: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    pytest.importorskip(adapter)
    module = importlib.import_module(f"foxglove.datasets.{adapter}")
    client = plan_client(
        [recording({"bucket": "bucket", "path": "file.mcap", "scheme": "gs"})]
    )
    store_factory = MagicMock()
    monkeypatch.setattr("foxglove.datasets.reader._CloudObjectStore", store_factory)
    if adapter == "ray":
        monkeypatch.setattr(
            module.ray.data, "read_datasource", lambda source, **kw: source
        )
    dataset = module.read_dataset(
        "dataset",
        version=7,
        topics=["/selected"],
        read_episode=first_sample,
        client_factory=lambda: client,
        source="object_storage",
    )
    assert dataset._plan.client_factory is None
    assert dataset._plan.object_store_factory is store_factory
    client.get_dataset_version_episodes.assert_called_once_with(
        dataset_id="dataset", version_number=7, limit=2000, include_recordings=True
    )
    store_factory.assert_not_called()


@pytest.mark.parametrize(
    "location,builtin",
    [
        pytest.param(
            ObjectLocation("bucket", "prefix/file.mcap", scheme="s3"),
            True,
            id="builtin-s3",
        ),
        pytest.param(
            ObjectLocation("bucket", "file.mcap"), False, id="custom-missing-scheme"
        ),
        pytest.param(
            ObjectLocation("bucket", "file.mcap", scheme="custom"),
            False,
            id="custom-unknown-scheme",
        ),
        pytest.param(
            ObjectLocation("container", "prefix/run.mcap", "account"),
            False,
            id="custom-azure",
        ),
    ],
)
def test_storage_planning_and_worker_lifecycle(
    location: ObjectLocation, builtin: bool, monkeypatch: pytest.MonkeyPatch
) -> None:
    raw_location = {
        "bucket": location.bucket,
        "path": location.path,
        "scheme": location.scheme,
    }
    if location.azure_storage_account_name:
        raw_location["azureStorageAccountName"] = location.azure_storage_account_name
    entry = recording(raw_location)
    client = plan_client([entry, entry])
    client_factory = MagicMock(return_value=client)
    store = Store({location: mcap_bytes([("/selected", 0, "sample")])})
    store_factory = MagicMock(return_value=store)
    builtin_factory = store_factory if builtin else MagicMock()
    monkeypatch.setattr("foxglove.datasets.reader._CloudObjectStore", builtin_factory)
    plan = _plan(
        "dataset",
        7,
        ["/selected"],
        client_factory,
        None if builtin else store_factory,
        source="object_storage",
    )
    assert plan.episodes[0].locations == (location,)
    assert plan.client_factory is None
    client.get_dataset_version_episodes.assert_called_once_with(
        dataset_id="dataset", version_number=7, limit=2000, include_recordings=True
    )
    store_factory.assert_not_called()
    assert list(plan.read(plan.episodes, first_sample)) == ["sample"]
    store_factory.assert_called_once_with()
    client_factory.assert_called_once_with()
    client.iter_messages.assert_not_called()
    assert store.locations == [location]
    assert store.files[0].closed
    if not builtin:
        builtin_factory.assert_not_called()


@pytest.mark.parametrize(
    "recordings,expected",
    [
        pytest.param(
            None, "Episode episode has no recording locations", id="missing-recordings"
        ),
        pytest.param(
            [], "Episode episode has no recording locations", id="empty-recordings"
        ),
        pytest.param(
            [recording(None)],
            "Recording recording in episode episode has no object location",
            id="missing-location",
        ),
        pytest.param(
            [recording(None, available=False)],
            "Recording recording in episode episode is not available",
            id="unavailable-without-location",
        ),
        pytest.param(
            [recording({"bucket": "b", "path": "p"}, available=False)],
            "Recording recording in episode episode is not available",
            id="unavailable-with-location",
        ),
        pytest.param(
            [recording({"bucket": "b"})],
            "Recording recording has an invalid object location",
            id="missing-path",
        ),
    ],
)
def test_invalid_recordings_fail_before_opening_storage(
    recordings: Any, expected: str
) -> None:
    client = plan_client(recordings)
    store_factory = MagicMock()
    with pytest.raises(ValueError, match=expected) as error:
        _plan(
            "dataset",
            7,
            ["/selected"],
            lambda: client,
            store_factory,
            source="object_storage",
        )
    store_factory.assert_not_called()
    if "not available" in expected:
        assert "client" not in str(error.value)


@pytest.mark.parametrize("adapter", ["plan", "torch", "ray"])
@pytest.mark.parametrize(
    "options,custom_store,expected",
    [
        pytest.param(
            {"source": "invalid"}, False, "source must be", id="invalid-source"
        ),
        pytest.param(
            {},
            True,
            "requires source='object_storage'",
            id="factory-with-default-source",
        ),
        pytest.param(
            {"source": "foxglove"},
            True,
            "requires source='object_storage'",
            id="factory-with-explicit-source",
        ),
    ],
)
def test_invalid_source_configuration_fails_before_factories(
    adapter: str, options: dict[str, Any], custom_store: bool, expected: str
) -> None:
    client_factory = MagicMock()
    store_factory = MagicMock()
    make_store = store_factory if custom_store else None
    if adapter != "plan":
        pytest.importorskip(adapter)
        module = importlib.import_module(f"foxglove.datasets.{adapter}")
    with pytest.raises(ValueError, match=expected):
        if adapter == "plan":
            _plan("dataset", 7, ["/camera"], client_factory, make_store, **options)
        else:
            module.read_dataset(
                "dataset",
                version=7,
                topics=["/camera"],
                read_episode=first_sample,
                client_factory=client_factory,
                object_store_factory=make_store,
                **options,
            )
    client_factory.assert_not_called()
    store_factory.assert_not_called()
