import argparse
import logging
import time
from typing import Optional

import foxglove
import mcap
import mcap.reader
import mcap.records
from foxglove import Channel, Schema
from foxglove.websocket import Capability, WebSocketServer

channels: dict[str, Channel] = {}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--file", type=str, required=True)
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--host", type=str, default="127.0.0.1")
    args = parser.parse_args()

    file_name = args.file

    server = foxglove.start_server(
        name=file_name, port=args.port, host=args.host, capabilities=[Capability.Time]
    )

    try:
        while True:
            stream_until_done(file_name, server)

            logging.info("Looping")
            server.clear_session()

    except KeyboardInterrupt:
        server.stop()


def stream_until_done(file_name: str, server: WebSocketServer):
    tracker: Optional[TimeTracker] = None
    with open(file_name, "rb") as f:
        reader = mcap.reader.make_reader(f)
        for mcap_schema, mcap_chan, mcap_msg in reader.iter_messages():
            if tracker is None:
                tracker = new_time_tracker(mcap_msg)

            tracker.sleep_until(mcap_msg.log_time)

            notify_time = tracker.notify()
            if notify_time is not None:
                server.broadcast_time(tracker._now_ns)

            channel = get_channel(mcap_schema, mcap_chan)
            channel.log(mcap_msg.data)


def new_time_tracker(message: mcap.records.Message):
    return TimeTracker(offset_ns=message.log_time)


def get_channel(
    mcap_schema: Optional[mcap.records.Schema], mcap_channel: mcap.records.Channel
) -> Channel:
    """
    Return a Channel we can log to, based on records seen in the mcap.
    Channels are stored based on topic name.
    """
    if mcap_channel.topic in channels:
        return channels[mcap_channel.topic]

    channels[mcap_channel.topic] = Channel(
        topic=mcap_channel.topic,
        message_encoding=mcap_channel.message_encoding,
        schema=(
            None
            if mcap_schema is None
            else Schema(
                name=mcap_schema.name,
                encoding=mcap_schema.encoding,
                data=mcap_schema.data,
            )
        ),
    )

    return channels[mcap_channel.topic]


class TimeTracker:
    """
    Helper for keep tracking of the relationship between a file timestamp and the wallclock.

    :param offset_ns: The offset from epoch to treat as "now".
    """

    def __init__(self, *, offset_ns: int):
        self._offset_ns = offset_ns
        self._now_ns = offset_ns
        self._notify_interval_ns = 1e9 / 60
        self._notify_last = 0
        self._start = time.time_ns()

    def sleep_until(self, offset_ns: int):
        elapsed = time.time_ns() - self._start
        delta = offset_ns - self._offset_ns - elapsed
        if delta > 0:
            time.sleep(delta / 1e9)
        self._now_ns = offset_ns

    def notify(self) -> Optional[int]:
        if self._now_ns - self._notify_last > self._notify_interval_ns:
            self._notify_last = self._now_ns
            return self._now_ns
        return None


if __name__ == "__main__":
    main()
