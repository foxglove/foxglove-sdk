"""Abstract base class for a data source that supports ranged playback.

Implement this with your own data format, then reuse the ServerListener
and main-loop structure from main.py in your own player application.
"""

from abc import ABC, abstractmethod

from foxglove.websocket import PlaybackStatus, WebSocketServer


class PlaybackSource(ABC):
    """A data source that supports ranged playback with play/pause, seek, and variable speed.

    Implementations are responsible for:
    - Tracking playback state (playing/paused/ended) and current position
    - Pacing message delivery according to timestamps and playback speed
    - Logging messages to channels and broadcasting time updates to the server
    """

    @abstractmethod
    def time_range(self) -> tuple[int, int]:
        """Returns the (start, end) time bounds of the data in nanoseconds.

        Determining this is dependent on the format of data you are loading.
        """
        ...

    @abstractmethod
    def play(self) -> None:
        """Begins or resumes playback.

        Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove.
        """
        ...

    @abstractmethod
    def pause(self) -> None:
        """Pauses playback.

        Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove.
        """
        ...

    @abstractmethod
    def seek(self, log_time: int) -> None:
        """Seeks to the specified timestamp in nanoseconds.

        Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove.
        """
        ...

    @abstractmethod
    def set_playback_speed(self, speed: float) -> None:
        """Sets the playback speed multiplier (e.g., 1.0 for real-time, 2.0 for double speed).

        Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove.
        """
        ...

    @abstractmethod
    def status(self) -> PlaybackStatus:
        """Returns the current playback status.

        Used to send a PlaybackState to Foxglove.
        """
        ...

    @abstractmethod
    def current_time(self) -> int:
        """Returns the current playback position in nanoseconds.

        Used to send a PlaybackState to Foxglove.
        """
        ...

    @abstractmethod
    def playback_speed(self) -> float:
        """Returns the current playback speed multiplier.

        Used to send a PlaybackState to Foxglove.
        """
        ...

    @abstractmethod
    def log_next_message(self, server: WebSocketServer) -> float | None:
        """Logs the next message for playback if it's ready, or returns a duration to wait.

        Returns seconds to sleep if the caller should wait before calling again,
        or None if a message was logged or playback is not active.

        The caller should sleep outside of any lock to allow control requests to be processed.
        This method also broadcasts time updates via server.broadcast_time().
        """
        ...
