from pathlib import Path
from typing import Callable, Generator, Optional

import pytest
from foxglove import Channel, ChannelDescriptor, Context, open_mcap
from foxglove.mcap import MCAPWriteOptions

chan = Channel("test", schema={"type": "object"})


@pytest.fixture
def make_tmp_mcap(
    tmp_path_factory: pytest.TempPathFactory,
) -> Generator[Callable[[], Path], None, None]:
    mcap: Optional[Path] = None
    dir: Optional[Path] = None

    def _make_tmp_mcap() -> Path:
        nonlocal dir, mcap
        dir = tmp_path_factory.mktemp("test", numbered=True)
        mcap = dir / "test.mcap"
        return mcap

    yield _make_tmp_mcap

    if mcap is not None and dir is not None:
        try:
            mcap.unlink()
            dir.rmdir()
        except FileNotFoundError:
            pass


@pytest.fixture
def tmp_mcap(make_tmp_mcap: Callable[[], Path]) -> Generator[Path, None, None]:
    yield make_tmp_mcap()


def test_open_with_str(tmp_mcap: Path) -> None:
    open_mcap(str(tmp_mcap))


def test_overwrite(tmp_mcap: Path) -> None:
    tmp_mcap.touch()
    with pytest.raises(FileExistsError):
        open_mcap(tmp_mcap)
    open_mcap(tmp_mcap, allow_overwrite=True)


def test_explicit_close(tmp_mcap: Path) -> None:
    mcap = open_mcap(tmp_mcap)
    for ii in range(20):
        chan.log({"foo": ii})
    size_before_close = tmp_mcap.stat().st_size
    mcap.close()
    assert tmp_mcap.stat().st_size > size_before_close


def test_context_manager(tmp_mcap: Path) -> None:
    with open_mcap(tmp_mcap):
        for ii in range(20):
            chan.log({"foo": ii})
        size_before_close = tmp_mcap.stat().st_size
    assert tmp_mcap.stat().st_size > size_before_close


def test_writer_compression(make_tmp_mcap: Callable[[], Path]) -> None:
    tmp_1 = make_tmp_mcap()
    tmp_2 = make_tmp_mcap()

    # Compression is enabled by default
    mcap_1 = open_mcap(tmp_1)
    mcap_2 = open_mcap(tmp_2, writer_options=MCAPWriteOptions(compression=None))

    for _ in range(20):
        chan.log({"foo": "bar"})

    mcap_1.close()
    mcap_2.close()

    assert tmp_1.stat().st_size < tmp_2.stat().st_size


def test_writer_custom_profile(tmp_mcap: Path) -> None:
    options = MCAPWriteOptions(profile="--custom-profile-1--")
    with open_mcap(tmp_mcap, writer_options=options):
        chan.log({"foo": "bar"})

    contents = tmp_mcap.read_bytes()
    assert contents.find(b"--custom-profile-1--") > -1


def test_write_to_different_contexts(make_tmp_mcap: Callable[[], Path]) -> None:
    tmp_1 = make_tmp_mcap()
    tmp_2 = make_tmp_mcap()

    ctx1 = Context()
    ctx2 = Context()

    options = MCAPWriteOptions(compression=None)
    mcap1 = open_mcap(tmp_1, writer_options=options, context=ctx1)
    mcap2 = open_mcap(tmp_2, writer_options=options, context=ctx2)

    ch1 = Channel("ctx1", context=ctx1)
    ch1.log({"a": "b"})

    ch2 = Channel("ctx2", context=ctx2)
    ch2.log({"has-more-data": "true"})

    mcap1.close()
    mcap2.close()

    contents1 = tmp_1.read_bytes()
    contents2 = tmp_2.read_bytes()

    assert len(contents1) < len(contents2)


def _verify_metadata_in_file(file_path: Path, expected_metadata: dict) -> None:
    """Helper function to verify metadata in MCAP file matches expected."""
    import mcap.reader

    with open(file_path, "rb") as f:
        reader = mcap.reader.make_reader(f)

        found_metadata = {}
        metadata_count = 0

        for record in reader.iter_metadata():
            metadata_count += 1
            found_metadata[record.name] = dict(record.metadata)

        # Verify count
        assert metadata_count == len(
            expected_metadata
        ), f"Expected {len(expected_metadata)} metadata records, found {metadata_count}"

        # Verify metadata names and content
        assert set(found_metadata.keys()) == set(
            expected_metadata.keys()
        ), "Metadata names don't match"

        for name, expected_kv in expected_metadata.items():
            assert (
                found_metadata[name] == expected_kv
            ), f"Metadata '{name}' has wrong key-value pairs"


def test_write_metadata(tmp_mcap: Path) -> None:
    """Test writing metadata to MCAP file."""
    # Define expected metadata
    expected_metadata = {
        "test1": {"key1": "value1", "key2": "value2"},
        "test2": {"a": "1", "b": "2"},
        "test3": {"x": "y", "z": "w"},
    }

    with open_mcap(tmp_mcap) as writer:
        # This should not raise an error
        writer.write_metadata("empty", {})

        # Write basic metadata
        writer.write_metadata("test1", expected_metadata["test1"])

        # Write multiple metadata records
        writer.write_metadata("test2", expected_metadata["test2"])
        writer.write_metadata("test3", expected_metadata["test3"])

        # Write empty metadata (should be skipped)
        writer.write_metadata("empty_test", {})

        # Log some messages
        for ii in range(5):
            chan.log({"foo": ii})

    # Verify metadata was written correctly
    _verify_metadata_in_file(tmp_mcap, expected_metadata)


def test_channel_filter(make_tmp_mcap: Callable[[], Path]) -> None:
    tmp_1 = make_tmp_mcap()
    tmp_2 = make_tmp_mcap()

    ch1 = Channel("/1", schema={"type": "object"})
    ch2 = Channel("/2", schema={"type": "object"})

    def filter(ch: ChannelDescriptor) -> bool:
        return ch.topic.startswith("/1")

    mcap1 = open_mcap(tmp_1, channel_filter=filter)
    mcap2 = open_mcap(tmp_2, channel_filter=None)

    ch1.log({})
    ch2.log({})

    mcap1.close()
    mcap2.close()

    assert tmp_1.stat().st_size < tmp_2.stat().st_size
