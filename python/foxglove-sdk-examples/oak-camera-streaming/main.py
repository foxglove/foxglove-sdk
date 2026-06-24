#!/usr/bin/env python3
"""
Stream a Luxonis OAK camera to Foxglove.

This tutorial example runs a DepthAI v3 pipeline on an OAK camera (e.g. the
OAK-4 D) and publishes the following live streams over the Foxglove WebSocket
server:

- ``/oak/rgb/image``         raw color video         (``foxglove.RawImage``)
- ``/oak/rgb/calibration``   intrinsics + distortion (``foxglove.CameraCalibration``)
- ``/oak/depth/image``       aligned depth (uint16)  (``foxglove.RawImage``, ``16UC1``)
- ``/oak/imu``               accelerometer + gyro    (JSON, ``sensor_msgs``-like)

Depth is aligned to the color camera on the device, so the depth image and
calibration all share CAM_A's optical frame and intrinsics. A rotation between
that optical frame and the upright "oak" frame is published on the ``/tf``
topic.

The code is organized in the same order you would write it:

1. Create one Foxglove channel per stream.
2. Build the DepthAI pipeline (color camera, stereo depth, IMU).
3. Loop: convert each DepthAI packet to a Foxglove message and log it.
"""

from __future__ import annotations

import argparse
import json
import logging
import time
from typing import Any

import depthai as dai
import foxglove
import numpy as np
from foxglove import Channel, Schema
from foxglove.channels import (
    CameraCalibrationChannel,
    FrameTransformsChannel,
    RawImageChannel,
)
from foxglove.messages import (
    CameraCalibration,
    FrameTransform,
    FrameTransforms,
    Quaternion,
    RawImage,
    Timestamp,
    Vector3,
)

# All messages are stamped with the camera's optical frame. A static transform
# (published below) relates it to the upright "oak" frame.
CAMERA_FRAME = "oak"
OPTICAL_FRAME = "oak_optical"

# JSON Schema for the IMU topic. Following the shape of sensor_msgs/Imu means
# Foxglove's Plot panel and existing ROS tooling understand the message.
IMU_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {
        "header": {
            "type": "object",
            "properties": {
                "stamp": {
                    "type": "object",
                    "properties": {
                        "sec": {"type": "integer"},
                        "nsec": {"type": "integer"},
                    },
                },
                "frame_id": {"type": "string"},
            },
        },
        "angular_velocity": {
            "type": "object",
            "properties": {
                "x": {"type": "number"},
                "y": {"type": "number"},
                "z": {"type": "number"},
            },
        },
        "linear_acceleration": {
            "type": "object",
            "properties": {
                "x": {"type": "number"},
                "y": {"type": "number"},
                "z": {"type": "number"},
            },
        },
    },
}

# Standard ROS rotation from a body frame (X forward, Z up) to a camera
# optical frame (Z forward, Y down). Publishing it lets the 3D panel render
# the optical-frame data upright when "oak" is the display frame.
OPTICAL_ROTATION = Quaternion(x=-0.5, y=0.5, z=-0.5, w=0.5)


def to_timestamp(td: Any) -> Timestamp:
    """Convert a DepthAI timestamp (a timedelta) to a Foxglove Timestamp."""
    try:
        total_ns = int(td.total_seconds() * 1e9)
        return Timestamp(sec=total_ns // 1_000_000_000, nsec=total_ns % 1_000_000_000)
    except Exception:
        return Timestamp.now()


# Foxglove supports a fixed set of distortion models. The DepthAI v3 Perspective
# model is OpenCV's 14-parameter rational polynomial; Foxglove's
# `rational_polynomial` uses only the first 8 (k1..k6, p1, p2). The Fisheye
# model maps to Kannala-Brandt with 4 coefficients.
_DISTORTION_MODELS = {
    "Perspective": ("rational_polynomial", 8),
    "Fisheye": ("kannala_brandt", 4),
}


def build_camera_calibration_kwargs(
    calib: dai.CalibrationHandler,
    socket: dai.CameraBoardSocket,
    width: int,
    height: int,
    frame_id: str,
) -> dict[str, Any]:
    """
    Pull intrinsics from a DepthAI calibration handler into the kwargs needed
    to build a Foxglove ``CameraCalibration``.

    Returns everything but ``timestamp`` — the caller stamps each message at
    publish time so MCAP replay can locate a recent calibration when scrubbing.
    """
    K = np.asarray(calib.getCameraIntrinsics(socket, width, height)).flatten().tolist()
    fx, _, cx, _, fy, cy, _, _, _ = K
    P = [
        fx, 0.0, cx, 0.0,
        0.0, fy, cy, 0.0,
        0.0, 0.0, 1.0, 0.0,
    ]
    R = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]

    model_name = str(calib.getDistortionModel(socket)).rsplit(".", 1)[-1]
    if model_name in _DISTORTION_MODELS:
        distortion_model, n_params = _DISTORTION_MODELS[model_name]
        D = list(map(float, calib.getDistortionCoefficients(socket)))[:n_params]
    else:
        logging.warning(
            "Unsupported DepthAI distortion model %r; publishing intrinsics only",
            model_name,
        )
        distortion_model = ""
        D = []

    return {
        "frame_id": frame_id,
        "width": width,
        "height": height,
        "distortion_model": distortion_model,
        "D": D,
        "K": K,
        "R": R,
        "P": P,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--rgb-width", type=int, default=1280, help="Color stream width"
    )
    parser.add_argument(
        "--rgb-height", type=int, default=720, help="Color stream height"
    )
    parser.add_argument("--fps", type=int, default=30, help="Camera frame rate")
    parser.add_argument(
        "--record", default="", help="Also record to an MCAP file at this path"
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")

    # ------------------------------------------------------------------
    # Step 1: Foxglove channels — one per stream.
    #
    # Channels with a well-known Foxglove schema (RawImage, CameraCalibration, ...)
    # have typed classes. The IMU uses a generic JSON channel with a
    # sensor_msgs-like schema.
    # ------------------------------------------------------------------
    rgb_channel = RawImageChannel(topic="/oak/rgb/image")
    cal_channel = CameraCalibrationChannel(topic="/oak/rgb/calibration")
    depth_channel = RawImageChannel(topic="/oak/depth/image")
    imu_channel = Channel(
        topic="/oak/imu",
        message_encoding="json",
        schema=Schema(
            name="sensor_msgs.msg.ImuLike",
            encoding="jsonschema",
            data=json.dumps(IMU_SCHEMA).encode("utf-8"),
        ),
    )
    tf_channel = FrameTransformsChannel(topic="/tf")

    # Optionally record everything that is logged to an MCAP file too.
    writer = foxglove.open_mcap(args.record) if args.record else None

    # Start the WebSocket server. Open https://app.foxglove.dev and connect
    # to ws://localhost:8765 (or follow the printed link).
    server = foxglove.start_server()
    logging.info("Foxglove server: %s", server.app_url())

    # ------------------------------------------------------------------
    # Step 2: DepthAI pipeline.
    #
    #   CAM_A (color) ── NV12 ──> host (raw video)
    #   CAM_B + CAM_C ──> StereoDepth ──> host (aligned depth)
    #   IMU ──> host
    # ------------------------------------------------------------------
    with dai.Pipeline() as pipeline:
        # Color camera: one NV12 stream; getCvFrame() converts it to BGR on
        # the host for the RawImage message.
        color = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_A)
        color_out = color.requestOutput(
            size=(args.rgb_width, args.rgb_height),
            type=dai.ImgFrame.Type.NV12,
            fps=args.fps,
        )
        rgb_queue = color_out.createOutputQueue(maxSize=2, blocking=False)

        # Stereo pair -> depth, all computed on the device.
        left = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_B)
        right = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_C)
        stereo = pipeline.create(dai.node.StereoDepth)
        left.requestOutput((640, 400), type=dai.ImgFrame.Type.GRAY8, fps=args.fps).link(
            stereo.left
        )
        right.requestOutput(
            (640, 400), type=dai.ImgFrame.Type.GRAY8, fps=args.fps
        ).link(stereo.right)
        stereo.setRectification(True)
        stereo.setLeftRightCheck(True)

        # Align depth to the color camera and emit it at the RGB resolution so
        # the depth image and CameraCalibration share one frame (CAM_A's
        # optical frame) and one set of intrinsics. ImageAlign works on both
        # RVC2 and RVC4 — on RVC4, StereoDepth.setOutputSize is not supported,
        # so the resize must happen in ImageAlign.
        align = pipeline.create(dai.node.ImageAlign)
        align.setOutputSize(args.rgb_width, args.rgb_height)
        stereo.depth.link(align.input)
        color_out.link(align.inputAlignTo)

        depth_queue = align.outputAligned.createOutputQueue(
            maxSize=2, blocking=False
        )

        # IMU: batch samples on the device so the host is not flooded with
        # tiny packets at 100 Hz.
        imu = pipeline.create(dai.node.IMU)
        imu.enableIMUSensor(dai.IMUSensor.ACCELEROMETER_UNCALIBRATED, 100)
        imu.enableIMUSensor(dai.IMUSensor.GYROSCOPE_UNCALIBRATED, 100)
        imu.setBatchReportThreshold(10)
        imu.setMaxBatchReports(40)
        imu_queue = imu.out.createOutputQueue(maxSize=50, blocking=False)

        pipeline.start()
        logging.info("OAK pipeline running — Ctrl+C to stop")

        # Read factory calibration for the color camera once. Intrinsics do
        # not change at runtime, so we cache the static fields and stamp a
        # fresh CameraCalibration per frame inside the loop.
        device = pipeline.getDefaultDevice()
        rgb_calibration_kwargs = build_camera_calibration_kwargs(
            device.readCalibration(),
            dai.CameraBoardSocket.CAM_A,
            args.rgb_width,
            args.rgb_height,
            OPTICAL_FRAME,
        )

        # --------------------------------------------------------------
        # Step 3: publish loop. tryGet() never blocks, so one loop can
        # service all queues at their own rates.
        # --------------------------------------------------------------
        try:
            while pipeline.isRunning():

                # Keep publishing the optical frame transform
                tf_channel.log(
                    FrameTransforms(
                        transforms=[
                            FrameTransform(
                                timestamp=Timestamp.now(),
                                parent_frame_id=CAMERA_FRAME,
                                child_frame_id=OPTICAL_FRAME,
                                translation=Vector3(x=0.0, y=0.0, z=0.0),
                                rotation=OPTICAL_ROTATION,
                            )
                        ]
                    )
                )

                frame = rgb_queue.tryGet()
                if isinstance(frame, dai.ImgFrame):
                    bgr = frame.getCvFrame()
                    height, width = bgr.shape[:2]
                    stamp = to_timestamp(frame.getTimestamp())
                    rgb_channel.log(
                        RawImage(
                            timestamp=stamp,
                            frame_id=OPTICAL_FRAME,
                            width=width,
                            height=height,
                            encoding="bgr8",
                            step=width * 3,
                            data=bgr.tobytes(),
                        )
                    )
                    # Re-emit the calibration with a fresh timestamp so MCAP
                    # replay can scrub to any time and still find a recent one.
                    cal_channel.log(
                        CameraCalibration(timestamp=stamp, **rgb_calibration_kwargs)
                    )

                depth_frame = depth_queue.tryGet()
                if isinstance(depth_frame, dai.ImgFrame):
                    depth = np.ascontiguousarray(
                        depth_frame.getFrame(), dtype=np.uint16
                    )
                    d_height, d_width = depth.shape[:2]
                    depth_channel.log(
                        RawImage(
                            timestamp=to_timestamp(depth_frame.getTimestamp()),
                            frame_id=OPTICAL_FRAME,
                            width=d_width,
                            height=d_height,
                            # StereoDepth's `depth` output is uint16 millimetres,
                            # which matches Foxglove's default depth scale for
                            # 16UC1 (0.001 → meters).
                            encoding="16UC1",
                            step=d_width * 2,
                            data=depth.tobytes(),
                        )
                    )

                imu_data = imu_queue.tryGet()
                if isinstance(imu_data, dai.IMUData):
                    for packet in imu_data.packets:
                        accel = packet.acceleroMeter
                        gyro = packet.gyroscope
                        stamp = to_timestamp(accel.getTimestamp())
                        imu_channel.log(
                            json.dumps(
                                {
                                    "header": {
                                        "stamp": {"sec": stamp.sec, "nsec": stamp.nsec},
                                        "frame_id": OPTICAL_FRAME,
                                    },
                                    "angular_velocity": {
                                        "x": float(gyro.x),
                                        "y": float(gyro.y),
                                        "z": float(gyro.z),
                                    },
                                    "linear_acceleration": {
                                        "x": float(accel.x),
                                        "y": float(accel.y),
                                        "z": float(accel.z),
                                    },
                                }
                            ).encode("utf-8")
                        )

                time.sleep(0.001)  # yield; all queues were just drained
        except KeyboardInterrupt:
            logging.info("Stopping…")
        finally:
            pipeline.stop()

    if writer is not None:
        writer.close()
        logging.info("MCAP written to %s", args.record)


if __name__ == "__main__":
    main()
