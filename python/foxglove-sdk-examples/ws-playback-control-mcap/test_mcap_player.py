import tempfile
import unittest
from math import isfinite
from pathlib import Path

import mcap.writer
from mcap_player import _MIN_PLAYBACK_SPEED, McapPlayer, TimeTracker, _clamp_speed


def _write_test_mcap(path: Path) -> None:
    with path.open("wb") as file_handle:
        writer = mcap.writer.Writer(file_handle)
        writer.start()
        schema_id = writer.register_schema("TestSchema", "jsonschema", b"{}")
        channel_id = writer.register_channel(
            topic="/test",
            message_encoding="json",
            schema_id=schema_id,
        )
        writer.add_message(
            channel_id=channel_id,
            log_time=10,
            publish_time=10,
            data=b"{}",
        )
        writer.finish()


class ClampSpeedTests(unittest.TestCase):
    def test_clamp_speed_rejects_positive_infinity(self) -> None:
        self.assertEqual(_clamp_speed(float("inf")), _MIN_PLAYBACK_SPEED)

    def test_time_tracker_handles_positive_infinity_speed(self) -> None:
        tracker = TimeTracker(offset_ns=0, speed=float("inf"))
        seconds_until = tracker.seconds_until(1_000)
        self.assertIsNotNone(seconds_until)
        if seconds_until is None:
            self.fail("seconds_until should return a finite delay")
        self.assertTrue(isfinite(seconds_until))


class McapPlayerCleanupTests(unittest.TestCase):
    def test_context_manager_closes_file_handle(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "test.mcap"
            _write_test_mcap(path)

            with McapPlayer(str(path)) as player:
                self.assertFalse(player._file.closed)

            self.assertTrue(player._file.closed)

    def test_seek_after_close_raises(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "test.mcap"
            _write_test_mcap(path)

            player = McapPlayer(str(path))
            player.close()

            with self.assertRaises(RuntimeError):
                player.seek(player.time_range()[0])


if __name__ == "__main__":
    unittest.main()
