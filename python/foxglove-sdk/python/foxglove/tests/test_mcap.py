from io import SEEK_CUR, SEEK_SET, BytesIO
from pathlib import Path
from typing import Callable, Generator, Optional, Union

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


def _verify_attachments_in_file(
    file_path: Path, expected_attachments: list[dict]
) -> None:
    """Helper function to verify attachments in MCAP file match expected."""
    import mcap.reader

    with open(file_path, "rb") as f:
        reader = mcap.reader.make_reader(f)

        found_attachments = []
        for attachment in reader.iter_attachments():
            found_attachments.append(
                {
                    "log_time": attachment.log_time,
                    "create_time": attachment.create_time,
                    "name": attachment.name,
                    "media_type": attachment.media_type,
                    "data": attachment.data,
                }
            )

        # Verify count
        assert len(found_attachments) == len(
            expected_attachments
        ), f"Expected {len(expected_attachments)} attachments, found {len(found_attachments)}"

        # Verify each attachment matches expected
        for expected in expected_attachments:
            matching = [a for a in found_attachments if a["name"] == expected["name"]]
            assert len(matching) == 1, f"Attachment '{expected['name']}' not found"
            actual = matching[0]
            assert (
                actual["log_time"] == expected["log_time"]
            ), f"Attachment '{expected['name']}' has wrong log_time"
            assert (
                actual["create_time"] == expected["create_time"]
            ), f"Attachment '{expected['name']}' has wrong create_time"
            assert (
                actual["media_type"] == expected["media_type"]
            ), f"Attachment '{expected['name']}' has wrong media_type"
            assert (
                actual["data"] == expected["data"]
            ), f"Attachment '{expected['name']}' has wrong data"


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


def test_attach_basic(tmp_mcap: Path) -> None:
    """Test writing a single attachment to MCAP file."""
    expected_attachments = [
        {
            "log_time": 1000000000,
            "create_time": 2000000000,
            "name": "config.json",
            "media_type": "application/json",
            "data": b'{"setting": true}',
        }
    ]

    with open_mcap(tmp_mcap) as writer:
        writer.attach(
            log_time=1000000000,
            create_time=2000000000,
            name="config.json",
            media_type="application/json",
            data=b'{"setting": true}',
        )

    _verify_attachments_in_file(tmp_mcap, expected_attachments)


def test_attach_multiple(tmp_mcap: Path) -> None:
    """Test writing multiple attachments to MCAP file."""
    expected_attachments = [
        {
            "log_time": 100,
            "create_time": 200,
            "name": "config.json",
            "media_type": "application/json",
            "data": b'{"setting": true}',
        },
        {
            "log_time": 300,
            "create_time": 400,
            "name": "calibration.yaml",
            "media_type": "text/yaml",
            "data": b"camera:\n  fx: 500\n  fy: 500",
        },
        {
            "log_time": 500,
            "create_time": 600,
            "name": "image.png",
            "media_type": "image/png",
            "data": bytes([0x89, 0x50, 0x4E, 0x47]),  # PNG magic bytes
        },
    ]

    with open_mcap(tmp_mcap) as writer:
        writer.attach(
            log_time=100,
            create_time=200,
            name="config.json",
            media_type="application/json",
            data=b'{"setting": true}',
        )
        writer.attach(
            log_time=300,
            create_time=400,
            name="calibration.yaml",
            media_type="text/yaml",
            data=b"camera:\n  fx: 500\n  fy: 500",
        )
        writer.attach(
            log_time=500,
            create_time=600,
            name="image.png",
            media_type="image/png",
            data=bytes([0x89, 0x50, 0x4E, 0x47]),  # PNG magic bytes
        )

    _verify_attachments_in_file(tmp_mcap, expected_attachments)


def test_attach_with_messages(tmp_mcap: Path) -> None:
    """Test writing attachments alongside messages."""
    with open_mcap(tmp_mcap) as writer:
        # Write some messages
        for ii in range(5):
            chan.log({"foo": ii})

        # Write an attachment
        writer.attach(
            log_time=1000,
            create_time=2000,
            name="notes.txt",
            media_type="text/plain",
            data=b"Recording notes",
        )

        # Write more messages
        for ii in range(5, 10):
            chan.log({"foo": ii})

    # Verify attachment was written
    expected_attachments = [
        {
            "log_time": 1000,
            "create_time": 2000,
            "name": "notes.txt",
            "media_type": "text/plain",
            "data": b"Recording notes",
        }
    ]
    _verify_attachments_in_file(tmp_mcap, expected_attachments)


def test_attach_after_close(tmp_mcap: Path) -> None:
    """Test that attaching after close raises an error."""
    writer = open_mcap(tmp_mcap)
    writer.close()

    with pytest.raises(Exception):  # FoxgloveError for SinkClosed
        writer.attach(
            log_time=100,
            create_time=200,
            name="test.txt",
            media_type="text/plain",
            data=b"test",
        )


# =============================================================================
# Tests for file-like object support
# =============================================================================


class TestFileLikeObject:
    """Tests for writing MCAP to file-like objects."""

    def test_write_to_bytesio(self) -> None:
        """Test writing MCAP to a BytesIO buffer produces valid MCAP."""
        buffer = BytesIO()
        test_chan = Channel("test_bytesio", schema={"type": "object"})

        with open_mcap(buffer):
            for i in range(10):
                test_chan.log({"value": i})

        # Verify buffer has data with MCAP magic bytes
        data = buffer.getvalue()
        assert len(data) > 0
        assert data[:8] == b"\x89MCAP0\r\n"

    def test_bytesio_readable_by_mcap_reader(self) -> None:
        """Test that MCAP written to BytesIO can be read back."""
        import mcap.reader

        buffer = BytesIO()
        test_chan = Channel("test_readable", schema={"type": "object"})

        with open_mcap(buffer):
            for i in range(5):
                test_chan.log({"index": i})

        # Read back and verify
        buffer.seek(0)
        reader = mcap.reader.make_reader(buffer)
        summary = reader.get_summary()

        assert summary is not None
        assert summary.statistics is not None
        assert summary.statistics.message_count == 5


# =============================================================================
# Tests for disable_seeking option
# =============================================================================


class NonSeekableWriter:
    """A file-like object that supports write/flush but raises on actual seeks.

    This mimics a non-seekable stream like a pipe or network socket. It allows
    position queries (seek(0, SEEK_CUR) or tell()) and no-op seeks to the current
    position, but raises OSError on any seek that would change the position.
    """

    def __init__(self) -> None:
        self._buffer = BytesIO()
        self._position = 0

    def write(self, data: Union[bytes, bytearray]) -> int:
        written = self._buffer.write(data)
        self._position += written
        return written

    def flush(self) -> None:
        self._buffer.flush()

    def seek(self, offset: int, whence: int = SEEK_SET) -> int:
        if whence == SEEK_CUR and offset == 0:
            # Allow querying current position (tell())
            return self._position
        elif whence == SEEK_SET and offset == self._position:
            # Allow no-op seek to current position
            return self._position
        else:
            # Actual seeks that change position are not supported
            raise OSError("Seeking is not supported")

    def getvalue(self) -> bytes:
        return self._buffer.getvalue()


class TestDisableSeeking:
    """Tests for the disable_seeking option in MCAPWriteOptions."""

    def test_disable_seeking_prevents_seek_calls(self) -> None:
        """Test that disable_seeking=True allows writing without seek calls."""
        writer = NonSeekableWriter()
        options = MCAPWriteOptions(disable_seeking=True)
        test_chan = Channel("test_no_seek", schema={"type": "object"})

        with open_mcap(writer, writer_options=options):
            test_chan.log({"value": 1})

        # Verify MCAP magic bytes are present
        assert writer.getvalue()[:8] == b"\x89MCAP0\r\n"

    def test_seeking_fails_without_disable_seeking(self) -> None:
        """Test that seeking is attempted by default and fails on non-seekable writer."""
        writer = NonSeekableWriter()
        test_chan = Channel("test_seek_default", schema={"type": "object"})

        with pytest.raises(RuntimeError):
            with open_mcap(writer):
                test_chan.log({"value": 1})
