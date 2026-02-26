#!/usr/bin/env python3
"""
Stream video and IMU from a Luxonis OAK camera to Foxglove Studio.

Requires a connected OAK device (e.g. OAK-D, OAK-D-Lite) with RGB camera (CAM_A) and IMU.
Video is published as RawImage at /camera/image; IMU as JSON at /imu for the Plot panel.
"""

import argparse
import signal
import sys
import time

import depthai as dai
import foxglove
from foxglove import Channel
from foxglove.channels import RawImageChannel
from foxglove.schemas import RawImage

# JSON schema for IMU messages (Plot panel)
IMU_SCHEMA = {
    "type": "object",
    "properties": {
        "timestamp": {"type": "number", "description": "Timestamp in seconds"},
        "accel_x": {"type": "number", "description": "Accelerometer X (m/s^2)"},
        "accel_y": {"type": "number", "description": "Accelerometer Y (m/s^2)"},
        "accel_z": {"type": "number", "description": "Accelerometer Z (m/s^2)"},
        "gyro_x": {"type": "number", "description": "Gyroscope X (rad/s)"},
        "gyro_y": {"type": "number", "description": "Gyroscope Y (rad/s)"},
        "gyro_z": {"type": "number", "description": "Gyroscope Z (rad/s)"},
    },
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Stream Luxonis OAK camera video and IMU to Foxglove",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "--resolution",
        type=str,
        default="640x400",
        help="Video resolution (e.g. 320x240, 640x400)",
    )
    parser.add_argument(
        "--fps",
        type=float,
        default=30.0,
        help="Camera frame rate (device must support this FPS for the chosen resolution)",
    )
    parser.add_argument(
        "--host",
        type=str,
        default="127.0.0.1",
        help="Foxglove WebSocket server host",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=8765,
        help="Foxglove WebSocket server port",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Print periodic stats (frame rate, IMU rate, timing) to find bottlenecks",
    )
    return parser.parse_args()


def parse_resolution(s: str) -> tuple[int, int]:
    try:
        w, h = s.strip().lower().split("x")
        return (int(w), int(h))
    except ValueError:
        raise ValueError(f"Invalid resolution {s!r}; use e.g. 640x400 or 320x240")


def main() -> int:
    args = parse_args()
    try:
        width, height = parse_resolution(args.resolution)
    except ValueError as e:
        print(e, file=sys.stderr)
        return 1

    # Build DepthAI pipeline: Camera (CAM_A) + IMU
    pipeline = dai.Pipeline()
    cam = pipeline.create(dai.node.Camera).build(
        dai.CameraBoardSocket.CAM_A,
        sensorFps=args.fps,
    )
    cam_out = cam.requestOutput((width, height), fps=args.fps)
    video_queue = cam_out.createOutputQueue()

    imu = pipeline.create(dai.node.IMU)
    imu.enableIMUSensor(dai.IMUSensor.ACCELEROMETER_RAW, 50)
    imu.enableIMUSensor(dai.IMUSensor.GYROSCOPE_RAW, 50)
    # Batch IMU packets so the host isn't flooded (avoids "reading too slowly" warnings)
    imu.setBatchReportThreshold(20)
    imu.setMaxBatchReports(20)
    imu_queue = imu.out.createOutputQueue(maxSize=50, blocking=False)

    # Start Foxglove server first so it's ready when data arrives
    server = foxglove.start_server(host=args.host, port=args.port)
    print(f"Foxglove server started at {server.app_url()}")

    image_channel = RawImageChannel(topic="/camera/image")
    imu_channel = Channel(topic="/imu", schema=IMU_SCHEMA)

    # Connect to device and start pipeline
    try:
        pipeline.start()
    except Exception as e:
        print("No OAK device found; connect a Luxonis camera (e.g. OAK-D, OAK-D-Lite).", file=sys.stderr)
        print(f"Error: {e}", file=sys.stderr)
        server.stop()
        return 1

    print(f"Camera: {width}x{height} @ {args.fps} fps. Streaming video and IMU. Press Ctrl+C to stop.")

    quit_event = False

    def set_quit(*_args: object) -> None:
        nonlocal quit_event
        quit_event = True

    signal.signal(signal.SIGINT, set_quit)
    signal.signal(signal.SIGTERM, set_quit)

    # Debug stats: time every operation in the loop
    debug = args.debug
    stats_interval = 2.0  # seconds
    last_stats_time = time.monotonic()
    frame_count = 0
    imu_packet_count = 0
    loop_iterations = 0
    # Accumulated times (ms) per stats interval
    t_video_drain = 0.0
    t_get_cv = 0.0
    t_tobytes = 0.0
    t_rawimage_build = 0.0
    t_log_image = 0.0
    t_imu_tryget = 0.0
    t_imu_log = 0.0
    t_sleep = 0.0
    t_loop_total = 0.0
    n_imu_batches = 0

    try:
        while pipeline.isRunning() and not quit_event:
            if debug:
                iter_start = time.perf_counter()

            # Video: drain queue and publish only the latest frame
            if debug:
                t0 = time.perf_counter()
            latest_frame = None
            while True:
                frame = video_queue.tryGet()
                if frame is None:
                    break
                latest_frame = frame
            if debug:
                t_video_drain += time.perf_counter() - t0

            if latest_frame is not None:
                assert isinstance(latest_frame, dai.ImgFrame)
                if debug:
                    t0 = time.perf_counter()
                bgr = latest_frame.getCvFrame()
                if debug:
                    t_get_cv += time.perf_counter() - t0
                h, w = bgr.shape[:2]
                if debug:
                    t0 = time.perf_counter()
                data = bgr.tobytes()
                if debug:
                    t_tobytes += time.perf_counter() - t0
                if debug:
                    t0 = time.perf_counter()
                msg = RawImage(
                    data=data,
                    width=w,
                    height=h,
                    step=w * 3,
                    encoding="bgr8",
                )
                if debug:
                    t_rawimage_build += time.perf_counter() - t0
                if debug:
                    t0 = time.perf_counter()
                image_channel.log(msg)
                if debug:
                    t_log_image += time.perf_counter() - t0
                frame_count += 1

            # IMU: drain all available packets
            if debug:
                t0 = time.perf_counter()
            imu_data_list = imu_queue.tryGetAll()
            if debug:
                t_imu_tryget += time.perf_counter() - t0
            if imu_data_list is not None:
                if debug:
                    n_imu_batches += len(imu_data_list)
                if debug:
                    t0 = time.perf_counter()
                for imu_batch in imu_data_list:
                    for packet in imu_batch.packets:
                        acc = packet.acceleroMeter
                        gyro = packet.gyroscope
                        ts = acc.getTimestamp()
                        if hasattr(ts, "total_seconds"):
                            timestamp_sec = ts.total_seconds()
                        else:
                            timestamp_sec = time.time()
                        imu_channel.log(
                            {
                                "timestamp": timestamp_sec,
                                "accel_x": acc.x,
                                "accel_y": acc.y,
                                "accel_z": acc.z,
                                "gyro_x": gyro.x,
                                "gyro_y": gyro.y,
                                "gyro_z": gyro.z,
                            }
                        )
                        imu_packet_count += 1
                if debug:
                    t_imu_log += time.perf_counter() - t0

            # No sleep: poll as fast as possible so we don't throttle frame or IMU delivery.
            if debug:
                t0 = time.perf_counter()
            time.sleep(0)  # yield to other threads only
            if debug:
                t_sleep += time.perf_counter() - t0
                t_loop_total += time.perf_counter() - iter_start
                loop_iterations += 1

            # Periodic debug stats
            if debug:
                now = time.monotonic()
                if now - last_stats_time >= stats_interval:
                    elapsed = now - last_stats_time
                    fps = frame_count / elapsed if elapsed > 0 else 0
                    imu_hz = imu_packet_count / elapsed if elapsed > 0 else 0
                    loops_per_s = loop_iterations / elapsed if elapsed > 0 else 0
                    print(
                        f"[stats] {elapsed:.1f}s: video {frame_count} frames ({fps:.1f} fps) | "
                        f"IMU {imu_packet_count} packets ({imu_hz:.0f} Hz, {n_imu_batches} batches) | "
                        f"loops {loop_iterations} ({loops_per_s:.0f}/s)"
                    )
                    print(
                        "        [ms] video_drain | getCvFrame | tobytes | RawImage_build | log_image | "
                        "imu_tryGet | imu_log | sleep | loop_total"
                    )
                    print(
                        f"        [ms] {t_video_drain*1000:8.1f} | {t_get_cv*1000:9.1f} | {t_tobytes*1000:6.1f} | "
                        f"{t_rawimage_build*1000:13.1f} | {t_log_image*1000:9.1f} | "
                        f"{t_imu_tryget*1000:9.1f} | {t_imu_log*1000:7.1f} | {t_sleep*1000:5.1f} | "
                        f"{t_loop_total*1000:10.1f}"
                    )
                    frame_count = 0
                    imu_packet_count = 0
                    loop_iterations = 0
                    t_video_drain = 0.0
                    t_get_cv = 0.0
                    t_tobytes = 0.0
                    t_rawimage_build = 0.0
                    t_log_image = 0.0
                    t_imu_tryget = 0.0
                    t_imu_log = 0.0
                    t_sleep = 0.0
                    t_loop_total = 0.0
                    n_imu_batches = 0
                    last_stats_time = now
    except KeyboardInterrupt:
        pass
    finally:
        pipeline.stop()
        pipeline.wait()
        server.stop()

    print("Stopped.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
