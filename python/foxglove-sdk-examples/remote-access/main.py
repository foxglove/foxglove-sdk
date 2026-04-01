import json
import logging
import math
import time

import foxglove
from foxglove import ChannelDescriptor
from foxglove.channels import CameraCalibrationChannel, RawImageChannel
from foxglove.messages import CameraCalibration, RawImage
from foxglove.remote_access import (
    Capability,
    Client,
    RemoteAccessConnectionStatus,
    RemoteAccessListener,
)

WIDTH = 480
HEIGHT = 270
BYTES_PER_PIXEL = 3
STEP = WIDTH * BYTES_PER_PIXEL
FX = 250.0
FY = 250.0
CX = WIDTH / 2.0
CY = HEIGHT / 2.0


class Listener(RemoteAccessListener):
    def on_connection_status_changed(
        self, status: RemoteAccessConnectionStatus
    ) -> None:
        logging.info(f"Connection status: {status.name}")

    def on_message_data(
        self,
        client: Client,
        channel: ChannelDescriptor,
        data: bytes,
    ) -> None:
        msg = json.loads(data)
        logging.info(f"Message from client {client.id} on topic {channel.topic}: {msg}")

    def on_client_advertise(self, client: Client, channel: ChannelDescriptor) -> None:
        logging.info(f"Client {client.id} advertised channel: {channel.topic}")

    def on_client_unadvertise(self, client: Client, channel: ChannelDescriptor) -> None:
        logging.info(f"Client {client.id} unadvertised channel: {channel.topic}")


def render_color_ramp(frame: int) -> bytes:
    """Render a scrolling vertical color ramp as an RGB8 image."""
    buf = bytearray(WIDTH * HEIGHT * BYTES_PER_PIXEL)
    for y in range(HEIGHT):
        brightness = y / HEIGHT
        for x in range(WIDTH):
            hue = ((x + frame * 2) / WIDTH * 360.0) % 360.0
            # HSV to RGB with S=1, V=brightness
            c = brightness
            h = hue / 60.0
            frac = h - math.floor(h)
            v = int(c * 255)
            p = 0
            q = int(c * (1.0 - frac) * 255)
            t = int(c * frac * 255)

            sector = int(h) % 6
            if sector == 0:
                r, g, b = v, t, p
            elif sector == 1:
                r, g, b = q, v, p
            elif sector == 2:
                r, g, b = p, v, t
            elif sector == 3:
                r, g, b = p, q, v
            elif sector == 4:
                r, g, b = t, p, v
            else:
                r, g, b = v, p, q

            off = y * STEP + x * BYTES_PER_PIXEL
            buf[off] = r
            buf[off + 1] = g
            buf[off + 2] = b
    return bytes(buf)


def main() -> None:
    foxglove.set_log_level(logging.INFO)

    gateway = foxglove.start_gateway(
        name="remote-access-example-python",
        capabilities=[Capability.ClientPublish],
        supported_encodings=["json"],
        listener=Listener(),
    )

    image_channel = RawImageChannel(topic="/camera/image")
    cal_channel = CameraCalibrationChannel(topic="/camera/calibration")

    calibration = CameraCalibration(
        frame_id="camera",
        width=WIDTH,
        height=HEIGHT,
        K=[FX, 0, CX, 0, FY, CY, 0, 0, 1],
        P=[FX, 0, CX, 0, 0, FY, CY, 0, 0, 0, 1, 0],
    )

    frame = 0
    try:
        while True:
            time.sleep(1 / 30)  # ~30 fps

            now_ns = time.time_ns()
            image_data = render_color_ramp(frame)

            image_channel.log(
                RawImage(
                    frame_id="camera",
                    width=WIDTH,
                    height=HEIGHT,
                    encoding="rgb8",
                    step=STEP,
                    data=image_data,
                ),
                log_time=now_ns,
            )

            cal_channel.log(calibration, log_time=now_ns)
            frame += 1
    except KeyboardInterrupt:
        logging.info("Shutting down...")
    finally:
        gateway.stop()
        logging.info("Done")


if __name__ == "__main__":
    main()
