#!/usr/bin/env python3
"""
RGB Camera Visualization with Foxglove SDK

This script captures video from a camera and streams it to Foxglove for visualization.
It supports writing data to MCAP files and configurable camera parameters.
"""

import argparse
import datetime
import logging
import time
from pathlib import Path
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

    # Camera configuration
    camera_group = parser.add_argument_group("camera", "Camera configuration")
    camera_group.add_argument(
        "--camera-id",
        type=str,
        default="0",
        help="Camera ID/path (e.g., 0 for /dev/video0, or full path like /dev/video1)",
    )
    camera_group.add_argument(
        "--width",
        type=int,
        default=640,
        help="Camera frame width",
    )
    camera_group.add_argument(
        "--height",
        type=int,
        default=480,
        help="Camera frame height",
    )
    camera_group.add_argument(
        "--fps",
        type=float,
        default=30.0,
        help="Camera frames per second",
    )
    camera_group.add_argument(
        "--topic",
        type=str,
        default="/camera/image_raw",
        help="Foxglove topic name for camera images",
    )

    # Output configuration
    output_group = parser.add_argument_group("output", "Output configuration")
    output_group.add_argument(
        "--write-mcap",
        action="store_true",
        help="Write data to MCAP file",
    )
    output_group.add_argument(
        "--mcap-path",
        type=str,
        help="Path for MCAP output file (auto-generated if not specified)",
    )

    # Display configuration
    display_group = parser.add_argument_group("display", "Display configuration")
    display_group.add_argument(
        "--show-preview",
        action="store_true",
        help="Show camera preview window (useful for debugging)",
    )

    return parser.parse_args()


class CameraCapture:
    """Handle camera capture using OpenCV."""

    def __init__(self, camera_id: str, width: int, height: int, fps: float):
        self.camera_id = camera_id
        self.width = width
        self.height = height
        self.fps = fps
        self.cap: Optional[cv2.VideoCapture] = None

    def connect(self) -> bool:
        """Connect to the camera."""
        try:
            # Try to parse camera_id as integer (for /dev/videoX)
            try:
                cam_id = int(self.camera_id)
            except ValueError:
                # Use as string path
                cam_id = self.camera_id

            self.cap = cv2.VideoCapture(cam_id)

            if not self.cap.isOpened():
                print(f"Failed to open camera {self.camera_id}")
                return False

            # Set camera properties
            self.cap.set(cv2.CAP_PROP_FRAME_WIDTH, self.width)
            self.cap.set(cv2.CAP_PROP_FRAME_HEIGHT, self.height)
            self.cap.set(cv2.CAP_PROP_FPS, self.fps)

            # Verify actual settings
            actual_width = int(self.cap.get(cv2.CAP_PROP_FRAME_WIDTH))
            actual_height = int(self.cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
            actual_fps = self.cap.get(cv2.CAP_PROP_FPS)

            print(f"Camera connected: {actual_width}x{actual_height} @ {actual_fps} fps")

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

        # Convert BGR to RGB (OpenCV uses BGR by default)
        frame_rgb = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
        return frame_rgb

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
        encoding="rgb8",
    )


def main():
    """Main function."""
    args = parse_args()

    # Set up logging
    foxglove.set_log_level(logging.INFO)

    print("Starting RGB Camera Visualization...")
    print(f"Camera ID: {args.camera_id}")
    print(f"Resolution: {args.width}x{args.height}")
    print(f"FPS: {args.fps}")
    print(f"Topic: {args.topic}")

    # Setup camera
    camera = CameraCapture(args.camera_id, args.width, args.height, args.fps)
    if not camera.connect():
        print("Failed to connect to camera. Exiting.")
        return 1

    # Setup MCAP writer if requested
    writer = None
    if args.write_mcap:
        if args.mcap_path:
            mcap_file = Path(args.mcap_path)
        else:
            timestamp = datetime.datetime.now().strftime("%Y-%m-%d_%H-%M-%S")
            mcap_file = Path(f"camera_feed_{timestamp}.mcap")

        print(f"Writing data to MCAP file: {mcap_file}")
        writer = foxglove.open_mcap(str(mcap_file))

    # Start Foxglove server
    server = foxglove.start_server()
    print(f"Foxglove server started at {server.app_url()}")

    # Create image channel
    image_channel = RawImageChannel(topic=args.topic)

    # Main loop
    frame_count = 0
    start_time = time.time()
    target_interval = 1.0 / args.fps

    try:
        print("Starting camera feed... Press Ctrl+C to stop.")

        while True:
            loop_start = time.time()

            # Capture frame
            frame = camera.read_frame()
            if frame is None:
                print("Failed to read frame from camera")
                continue

            # Create and publish message
            img_msg = create_raw_image_message(frame)
            image_channel.log(img_msg)

            # Show preview if requested
            if args.show_preview:
                # Convert back to BGR for OpenCV display
                frame_bgr = cv2.cvtColor(frame, cv2.COLOR_RGB2BGR)
                cv2.imshow("Camera Preview", frame_bgr)
                if cv2.waitKey(1) & 0xFF == ord('q'):
                    break

            frame_count += 1

            # Print statistics every 100 frames
            if frame_count % 100 == 0:
                elapsed = time.time() - start_time
                actual_fps = frame_count / elapsed
                print(f"Frames: {frame_count}, Actual FPS: {actual_fps:.1f}")

            # Sleep to maintain target FPS
            loop_duration = time.time() - loop_start
            sleep_time = max(0, target_interval - loop_duration)
            if sleep_time > 0:
                time.sleep(sleep_time)

    except KeyboardInterrupt:
        print("\nShutting down camera visualization...")

    except Exception as e:
        print(f"Error during execution: {e}")
        return 1

    finally:
        # Cleanup
        camera.disconnect()
        server.stop()

        if args.show_preview:
            cv2.destroyAllWindows()

        if writer:
            writer.close()
            print("MCAP file saved successfully.")

        # Final statistics
        if frame_count > 0:
            total_time = time.time() - start_time
            avg_fps = frame_count / total_time
            print(f"Session complete: {frame_count} frames in {total_time:.1f}s (avg {avg_fps:.1f} FPS)")

    return 0


if __name__ == "__main__":
    exit(main())
