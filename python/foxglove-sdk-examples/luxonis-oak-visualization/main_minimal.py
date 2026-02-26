#!/usr/bin/env python3
"""
Minimal script: camera only at 30 Hz, no IMU, no Foxglove, no OpenCV.

Use this to verify whether the OAK device delivers 30 fps with only the camera
in the pipeline. Run and check the printed FPS; Ctrl+C to quit.

  python main_minimal.py

Requires: depthai only.
"""

import sys
import time

import depthai as dai

# Same camera config as main.py (CAM_A, 30 fps, 640x400)
WIDTH = 640
HEIGHT = 400
FPS = 30.0


def main() -> int:
    pipeline = dai.Pipeline(dai.Device(dai.DeviceInfo("169.254.237.175")))
    #pipeline = dai.Pipeline()
    # 169.254.237.175
    
    cam = pipeline.create(dai.node.Camera).build(
        dai.CameraBoardSocket.CAM_A,
        sensorFps=FPS,
    )
    cam_out = cam.requestOutput((WIDTH, HEIGHT), fps=FPS)
    video_queue = cam_out.createOutputQueue()

    try:
        pipeline.start()
    except Exception as e:
        print("No OAK device found. Connect a Luxonis camera and try again.", file=sys.stderr)
        print(f"Error: {e}", file=sys.stderr)
        return 1

    print(f"Camera: {WIDTH}x{HEIGHT} @ {FPS} fps. Press 'q' in the window to quit.")
    print("Reported FPS is printed every 2 seconds.\n")

    frame_count = 0
    last_stats = time.monotonic()

    try:
        while pipeline.isRunning():
            frame = video_queue.get()
            assert isinstance(frame, dai.ImgFrame)
            _ = frame.getCvFrame()  # consume so we measure actual delivery rate
            frame_count += 1

            now = time.monotonic()
            if now - last_stats >= 2.0:
                elapsed = now - last_stats
                fps = frame_count / elapsed
                print(f"[stats] {elapsed:.1f}s: {frame_count} frames = {fps:.1f} fps")
                frame_count = 0
                last_stats = now

    except KeyboardInterrupt:
        pass
    finally:
        pipeline.stop()
        pipeline.wait()

    print("Stopped.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
