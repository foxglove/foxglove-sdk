"""
Regression tests for GIL deadlock when creating channels or opening MCAP writers
with a channel_filter registered on a context sink.

These tests run in subprocesses with a timeout to detect deadlocks without
hanging the test suite. The McapSink is registered directly as a context sink
with a filter, so should_subscribe() is invoked synchronously during
add_channel() and add_sink() without needing a connected websocket client.
"""

import subprocess
import sys


def test_channel_creation_with_mcap_channel_filter() -> None:
    """
    Creating a Channel while an MCAP writer with a channel_filter is open
    must not deadlock. The build_raw() -> add_channel() path notifies the
    MCAP sink, which calls should_subscribe().
    """
    script = """\
import tempfile, pathlib, foxglove

ctx = foxglove.Context()
path = pathlib.Path(tempfile.mktemp(suffix=".mcap"))
mcap = foxglove.open_mcap(path, context=ctx, channel_filter=lambda ch: True)
ch = foxglove.Channel("/test", context=ctx)
ch.log({"ok": True})
mcap.close()
path.unlink()

print("test_complete")
"""
    result = subprocess.run(
        [sys.executable, "-c", script],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode == 0, f"stderr: {result.stderr}"
    assert "test_complete" in result.stdout


def test_open_mcap_with_channel_filter_and_existing_channels() -> None:
    """
    Opening an MCAP writer with a channel_filter when channels already exist
    must not deadlock. The create() -> add_sink() path calls should_subscribe()
    on every existing channel.
    """
    script = """\
import tempfile, pathlib, foxglove

ctx = foxglove.Context()
ch = foxglove.Channel("/test", context=ctx)

path = pathlib.Path(tempfile.mktemp(suffix=".mcap"))
mcap = foxglove.open_mcap(path, context=ctx, channel_filter=lambda ch: True)
ch.log({"ok": True})
mcap.close()
path.unlink()

print("test_complete")
"""
    result = subprocess.run(
        [sys.executable, "-c", script],
        capture_output=True,
        text=True,
        timeout=10,
    )
    assert result.returncode == 0, f"stderr: {result.stderr}"
    assert "test_complete" in result.stdout
