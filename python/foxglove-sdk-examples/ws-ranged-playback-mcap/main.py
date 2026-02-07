"""Streams an MCAP file over a WebSocket with ranged playback support.

This example demonstrates how to implement ranged playback using the Foxglove SDK.
The PlaybackSource ABC lets you swap in your own data format while reusing the
ServerListener and main-loop structure below.
"""

import argparse
import logging
import threading
import time

import foxglove
from foxglove.websocket import (
    Capability,
    PlaybackCommand,
    PlaybackControlRequest,
    PlaybackState,
    PlaybackStatus,
    ServerListener,
)

from mcap_player import McapPlayer
from playback_source import PlaybackSource

logger = logging.getLogger(__name__)


class Listener(ServerListener):
    """Responds to PlaybackControlRequests from Foxglove.

    Processes the fields in the request (seeking, updating the playback speed,
    and handling play/pause commands) by calling methods on the PlaybackSource,
    then queries it to build the PlaybackState sent back to Foxglove.

    The intent of PlaybackSource is to let you implement the ABC with your own
    data format, then reuse this Listener in your own player application.
    """

    def __init__(
        self, player: PlaybackSource, lock: threading.Lock
    ) -> None:
        self._player = player
        self._lock = lock

    def on_playback_control_request(
        self, playback_control_request: PlaybackControlRequest
    ) -> PlaybackState:
        with self._lock:
            player = self._player

            # Setting did_seek to true clears panels in the Foxglove player. For
            # simplicity, we set this every time a seek is requested from Foxglove.
            # In your application, consider implementing logic that determines whether
            # a seek represents a significant jump in time.
            did_seek = playback_control_request.seek_time is not None

            if playback_control_request.seek_time is not None:
                try:
                    player.seek(playback_control_request.seek_time)
                except Exception as e:
                    did_seek = False
                    logger.warning("Failed to seek: %s", e)

            player.set_playback_speed(playback_control_request.playback_speed)

            if playback_control_request.playback_command == PlaybackCommand.Play:
                player.play()
            elif playback_control_request.playback_command == PlaybackCommand.Pause:
                player.pause()

            return PlaybackState(
                current_time=player.current_time(),
                playback_speed=player.playback_speed(),
                status=player.status(),
                did_seek=did_seek,
                request_id=playback_control_request.request_id,
            )


def main() -> None:
    logging.basicConfig(level=logging.INFO)

    parser = argparse.ArgumentParser(
        description="Stream an MCAP file with ranged playback"
    )
    parser.add_argument("--file", type=str, required=True, help="MCAP file to read")
    parser.add_argument("--port", type=int, default=8765, help="Server TCP port")
    parser.add_argument(
        "--host", type=str, default="127.0.0.1", help="Server IP address"
    )
    args = parser.parse_args()

    logger.info("Loading MCAP summary")
    player = McapPlayer(args.file)
    start_time, end_time = player.time_range()

    lock = threading.Lock()
    listener = Listener(player, lock)

    server = foxglove.start_server(
        name=args.file,
        host=args.host,
        port=args.port,
        capabilities=[Capability.RangedPlayback, Capability.Time],
        playback_time_range=(start_time, end_time),
        server_listener=listener,
    )

    logger.info("Server started, waiting for client")

    try:
        last_status = PlaybackStatus.Paused
        while True:
            # Check status and broadcast Ended state change
            with lock:
                status = player.status()
                if (
                    status == PlaybackStatus.Ended
                    and last_status != PlaybackStatus.Ended
                ):
                    server.broadcast_playback_state(
                        PlaybackState(
                            current_time=player.current_time(),
                            playback_speed=player.playback_speed(),
                            status=status,
                            did_seek=False,
                            request_id=None,
                        )
                    )
            last_status = status

            if status != PlaybackStatus.Playing:
                time.sleep(0.01)
                continue

            # Log next message, sleeping outside the lock if needed
            with lock:
                sleep_duration = player.log_next_message(server)

            if sleep_duration is not None:
                # Cap sleep to 1 second to keep the player responsive
                time.sleep(min(sleep_duration, 1.0))

    except KeyboardInterrupt:
        pass

    server.stop()


if __name__ == "__main__":
    main()
