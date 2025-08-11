import argparse
from typing import Optional

import cv2
import foxglove
import numpy as np
from foxglove.channels import RawImageChannel
from foxglove.schemas import RawImage


def parse_args() -> argparse.Namespace:
    """Parse command line arguments."""
    parser = argparse.ArgumentParser(
        description="RGB Camera visualization with Foxglove",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    camera_group = parser.add_argument_group("camera", "Camera configuration")
    camera_group.add_argument(
        "--camera-id",
        type=str,
        default="0",
        help="Camera ID/path",
    )

    return parser.parse_args()


class CameraCapture:
    def __init__(self, camera_id: str):
        self.camera_id = camera_id
        self.cap: Optional[cv2.VideoCapture] = None

    def connect(self) -> bool:
        try:
            try:
                cam_id = int(self.camera_id)
            except ValueError:
                cam_id = self.camera_id

            self.cap = cv2.VideoCapture(cam_id)

            if not self.cap.isOpened():
                print(f"Failed to open camera {self.camera_id}")
                return False

            actual_width = int(self.cap.get(cv2.CAP_PROP_FRAME_WIDTH))
            actual_height = int(self.cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
            actual_fps = self.cap.get(cv2.CAP_PROP_FPS)

            print("Camera connected successfully:")
            print(f"  ID/Path: {cam_id}")
            print(f"  Resolution: {actual_width} x {actual_height}")
            print(f"  Frame Rate: {actual_fps:.1f} fps")

            return True

        except Exception as e:
            print(f"Error connecting to camera: {e}")
            return False

    def read_frame(self) -> Optional[np.ndarray]:
        """Read a frame from the camera."""
        if not self.cap:
            return None

        ret, frame = self.cap.read()
        if not ret:
            return None

        return frame

    def disconnect(self):
        """Disconnect from the camera."""
        if self.cap:
            self.cap.release()
            self.cap = None


def create_raw_image_message(frame: np.ndarray) -> RawImage:
    """Convert numpy array to Foxglove RawImage message."""
    height, width, channels = frame.shape

    return RawImage(
        data=frame.tobytes(),
        width=width,
        height=height,
        step=width * channels,  # bytes per row
        encoding="bgr8",  # OpenCV default
    )


def main():
    args = parse_args()

    camera = CameraCapture(args.camera_id)
    if not camera.connect():
        print("Failed to connect to camera. Exiting.")
        return 0

    server = foxglove.start_server()
    print(f"Foxglove server started at {server.app_url()}")

    # Create image channel
    image_channel = RawImageChannel(topic="/camera/image")

    try:
        print("Starting camera feed... Press Ctrl+C to stop.")

        while True:
            frame = camera.read_frame()
            if frame is None:
                print("Failed to read frame from camera")
                continue

            img_msg = create_raw_image_message(frame)
            image_channel.log(img_msg)

    except KeyboardInterrupt:
        print("\nShutting down camera visualization...")

    except Exception as e:
        print(f"Error during execution: {e}")
        return 1

    finally:
        camera.disconnect()
        server.stop()

    return 0


if __name__ == "__main__":
    exit(main())
