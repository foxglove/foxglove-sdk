"""MCAP implementation of PlaybackSource.

Reads an MCAP file and plays back its messages with timing, supporting
play/pause, seek, and variable playback speed.
"""

import time
from typing import Iterator, Optional

import mcap.reader
import mcap.records
from foxglove import Channel, Schema
from foxglove.websocket import PlaybackStatus, WebSocketServer

from playback_source import PlaybackSource

_MIN_PLAYBACK_SPEED = 0.01


class McapPlayer(PlaybackSource):
    def __init__(self, path: str):
        self._path = path
        self._channels: dict[str, Channel] = {}
        self._time_tracker: Optional[TimeTracker] = None
        self._status = PlaybackStatus.Paused
        self._playback_speed = 1.0

        # Read summary to get time range
        with open(path, "rb") as f:
            reader = mcap.reader.make_reader(f)
            summary = reader.get_summary()
            if summary is None or summary.statistics is None:
                raise RuntimeError("MCAP file is missing summary/statistics")
            stats = summary.statistics
            self._time_range = (stats.message_start_time, stats.message_end_time)
            self._current_time = stats.message_start_time

        # Set up the message iterator
        self._file = open(path, "rb")
        self._reader = mcap.reader.make_reader(self._file)
        self._iter: Iterator[
            tuple[
                Optional[mcap.records.Schema],
                mcap.records.Channel,
                mcap.records.Message,
            ]
        ] = self._reader.iter_messages(start_time=self._current_time)
        self._pending: Optional[
            tuple[
                Optional[mcap.records.Schema],
                mcap.records.Channel,
                mcap.records.Message,
            ]
        ] = None

    def _reset_reader(self, start_time: int) -> None:
        """Re-open the MCAP reader starting from the given time."""
        self._file.close()
        self._file = open(self._path, "rb")
        self._reader = mcap.reader.make_reader(self._file)
        self._iter = self._reader.iter_messages(start_time=start_time)
        self._current_time = start_time
        self._time_tracker = None
        self._pending = None

    def _next_message(
        self,
    ) -> Optional[
        tuple[
            Optional[mcap.records.Schema],
            mcap.records.Channel,
            mcap.records.Message,
        ]
    ]:
        """Returns the next message, consuming any buffered pending message first."""
        if self._pending is not None:
            msg = self._pending
            self._pending = None
            return msg
        return next(self._iter, None)

    def _get_channel(
        self,
        mcap_schema: Optional[mcap.records.Schema],
        mcap_channel: mcap.records.Channel,
    ) -> Channel:
        """Return a Channel for logging, creating it from MCAP records if needed."""
        if mcap_channel.topic in self._channels:
            return self._channels[mcap_channel.topic]

        schema = None
        if mcap_schema is not None:
            schema = Schema(
                name=mcap_schema.name,
                encoding=mcap_schema.encoding,
                data=mcap_schema.data,
            )

        channel = Channel(
            topic=mcap_channel.topic,
            message_encoding=mcap_channel.message_encoding,
            schema=schema,
        )
        self._channels[mcap_channel.topic] = channel
        return channel

    # --- PlaybackSource implementation ---

    def time_range(self) -> tuple[int, int]:
        return self._time_range

    def status(self) -> PlaybackStatus:
        return self._status

    def current_time(self) -> int:
        return self._current_time

    def playback_speed(self) -> float:
        return self._playback_speed

    def set_playback_speed(self, speed: float) -> None:
        speed = _clamp_speed(speed)
        if self._time_tracker is not None:
            self._time_tracker.set_speed(speed)
        self._playback_speed = speed

    def play(self) -> None:
        # Don't transition to Playing if playback has ended.
        # To restart playback, the caller must seek first.
        if self._status == PlaybackStatus.Ended:
            return
        if self._time_tracker is not None:
            self._time_tracker.resume()
        self._status = PlaybackStatus.Playing

    def pause(self) -> None:
        if self._time_tracker is not None:
            self._time_tracker.pause()
        self._status = PlaybackStatus.Paused

    def seek(self, log_time: int) -> None:
        log_time = max(self._time_range[0], min(log_time, self._time_range[1]))
        self._reset_reader(log_time)
        # If playback had ended, reset to Paused so play() can transition to Playing
        if self._status == PlaybackStatus.Ended:
            self._status = PlaybackStatus.Paused

    def log_next_message(self, server: WebSocketServer) -> float | None:
        if self._status != PlaybackStatus.Playing:
            return None

        record = self._next_message()
        if record is None:
            # No more messages, playback has ended
            self._status = PlaybackStatus.Ended
            self._current_time = self._time_range[1]
            return None

        mcap_schema, mcap_channel, mcap_msg = record

        # Create TimeTracker on first message after starting playback
        if self._time_tracker is None:
            self._time_tracker = TimeTracker(
                offset_ns=mcap_msg.log_time, speed=self._playback_speed
            )

        # Check if we need to wait before emitting this message
        sleep_secs = self._time_tracker.seconds_until(mcap_msg.log_time)
        if sleep_secs is not None and sleep_secs > 0:
            # Buffer the message and return the sleep duration
            self._pending = record
            return sleep_secs

        # Update current time
        self._current_time = mcap_msg.log_time

        # Broadcast time update periodically
        notify_time = self._time_tracker.notify(mcap_msg.log_time)
        if notify_time is not None:
            server.broadcast_time(notify_time)

        # Log the message to the appropriate channel
        channel = self._get_channel(mcap_schema, mcap_channel)
        channel.log(mcap_msg.data, log_time=mcap_msg.log_time)

        return None


def _clamp_speed(speed: float) -> float:
    if not (speed >= _MIN_PLAYBACK_SPEED):  # handles NaN/inf
        return _MIN_PLAYBACK_SPEED
    return speed


class TimeTracker:
    """Tracks the relationship between wall-clock time and playback log time.

    Supports pause/resume and variable playback speed.
    """

    def __init__(self, *, offset_ns: int, speed: float):
        self._start_ns = time.monotonic_ns()
        self._offset_ns = offset_ns
        self._speed = _clamp_speed(speed)
        self._paused = False
        self._paused_elapsed_ns = 0
        self._notify_interval_ns = 1_000_000_000 // 60  # ~60 Hz
        self._notify_last = 0

    def _current_log_time(self) -> int:
        """Returns the current playback position in log-time nanoseconds."""
        if self._paused:
            return self._offset_ns + self._paused_elapsed_ns
        elapsed_wall_ns = time.monotonic_ns() - self._start_ns
        elapsed_log_ns = int(elapsed_wall_ns * self._speed)
        return self._offset_ns + self._paused_elapsed_ns + elapsed_log_ns

    def seconds_until(self, log_time: int) -> float | None:
        """Returns seconds to wait before log_time is ready, or None if ready now."""
        current = self._current_log_time()
        if log_time <= current:
            return None
        log_diff_ns = log_time - current
        if self._speed > 0:
            wall_diff_ns = log_diff_ns / self._speed
        else:
            wall_diff_ns = 1_000_000_000  # 1 second if speed is 0
        return wall_diff_ns / 1e9

    def pause(self) -> None:
        if not self._paused:
            elapsed_wall_ns = time.monotonic_ns() - self._start_ns
            self._paused_elapsed_ns += int(elapsed_wall_ns * self._speed)
            self._paused = True

    def resume(self) -> None:
        if self._paused:
            self._start_ns = time.monotonic_ns()
            self._paused = False

    def set_speed(self, speed: float) -> None:
        speed = _clamp_speed(speed)
        if not self._paused:
            # Accumulate elapsed time at the old speed before changing
            elapsed_wall_ns = time.monotonic_ns() - self._start_ns
            self._paused_elapsed_ns += int(elapsed_wall_ns * self._speed)
            self._start_ns = time.monotonic_ns()
        self._speed = speed

    def notify(self, current_ns: int) -> int | None:
        """Returns a timestamp to broadcast at ~60 Hz, or None."""
        if current_ns - self._notify_last >= self._notify_interval_ns:
            self._notify_last = current_ns
            return current_ns
        return None
