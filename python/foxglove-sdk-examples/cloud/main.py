import time

import foxglove
from foxglove.schemas import RawImage


class MessageHandler(foxglove.CloudSinkListener):
    def __init__(self) -> None:
        self.topics_by_channel_id: dict[int, str] = {}

    def on_client_advertise(
        self,
        client: foxglove.websocket.Client,
        client_channel: foxglove.websocket.ClientChannel,
    ) -> None:
        self.topics_by_channel_id[client_channel.id] = client_channel.topic

    # Called when a connected app publishes a message, such as from the Teleop panel.
    def on_message_data(
        self,
        client: foxglove.websocket.Client,
        client_channel_id: int,
        data: bytes,
    ) -> None:
        topic = self.topics_by_channel_id.get(client_channel_id, "unknown topic")
        print(f"Teleop message from {client.id} on {topic}: {data!r}")


def main() -> None:
    foxglove.set_log_level("DEBUG")

    # Connect to Foxglove Agent for live visualization
    handle = foxglove.start_cloud_sink(listener=MessageHandler())

    try:
        run_camera_loop()
    except KeyboardInterrupt:
        print("\nShutting down...")
    finally:
        handle.stop()


def run_camera_loop() -> None:
    offset = 0
    width = 960
    height = 540

    while True:
        data = gradient_data(width, height, offset)
        img = RawImage(
            width=width,
            height=height,
            encoding="bgr8",
            step=(width * 3),
            data=data,
        )
        foxglove.log("/camera", img)

        offset = (offset + 1) % width
        time.sleep(0.33)


def gradient_data(width: int, height: int, offset: int) -> bytes:
    data = bytearray(width * height * 3)
    for y in range(height):
        for x in range(width):
            idx = (y * width + x) * 3
            shifted_x = (x + offset) % width
            gradient = shifted_x * 255 // width

            # B, G, R
            data[idx] = gradient
            data[idx + 1] = 255 - gradient
            data[idx + 2] = gradient // 2

    return bytes(data)


if __name__ == "__main__":
    main()
