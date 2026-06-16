#!/usr/bin/env python3
"""
Stream a Luxonis OAK camera to Foxglove.

This tutorial example runs a DepthAI v3 pipeline on an OAK camera (e.g. the
OAK-4 D) and publishes three live streams over the Foxglove WebSocket server:

- ``/oak/rgb/image``   raw color video      (``foxglove.RawImage``)
- ``/oak/points``      stereo point cloud   (``foxglove.PointCloud``)
- ``/oak/imu``         accelerometer + gyro (JSON, ``sensor_msgs``-like)

A single static transform on ``/tf`` orients the camera's optical frame so the
point cloud appears upright in Foxglove's 3D panel.

The code is organized in the same order you would write it:

1. Create one Foxglove channel per stream.
2. Build the DepthAI pipeline (color camera, stereo depth -> point cloud, IMU).
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
    FrameTransformsChannel,
    PointCloudChannel,
    RawImageChannel,
)
from foxglove.messages import (
    FrameTransform,
    FrameTransforms,
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Pose,
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

# A Foxglove PointCloud is a packed binary buffer plus a description of its
# layout: here, three consecutive float32 values (x, y, z) per point.
POINT_STRIDE = 12  # 3 * sizeof(float32)
POINT_FIELDS = [
    PackedElementField(name="x", offset=0, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="y", offset=4, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="z", offset=8, type=PackedElementFieldNumericType.Float32),
]
IDENTITY_POSE = Pose(
    position=Vector3(x=0.0, y=0.0, z=0.0),
    orientation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
)

# Standard ROS rotation from a body frame (X forward, Z up) to a camera
# optical frame (Z forward, Y down). Publishing it lets the 3D panel render
# the optical-frame point cloud upright when "oak" is the display frame.
OPTICAL_ROTATION = Quaternion(x=-0.5, y=0.5, z=-0.5, w=0.5)


def to_timestamp(td: Any) -> Timestamp:
    """Convert a DepthAI timestamp (a timedelta) to a Foxglove Timestamp."""
    try:
        total_ns = int(td.total_seconds() * 1e9)
        return Timestamp(sec=total_ns // 1_000_000_000, nsec=total_ns % 1_000_000_000)
    except Exception:
        return Timestamp.now()


def point_cloud_to_msg(
    pcl_data: dai.PointCloudData, scale_to_meters: float
) -> PointCloud | None:
    """Convert device-generated ``dai.PointCloudData`` to ``foxglove.PointCloud``."""
    points = pcl_data.getPoints()  # (N, 3) float32 array
    if not isinstance(points, np.ndarray) or points.size == 0:
        return None
    xyz = points.reshape(-1, points.shape[-1])[:, :3]
    xyz = xyz[np.isfinite(xyz).all(axis=1)]
    if xyz.size == 0:
        return None
    xyz = xyz.astype(np.float32, copy=False) * np.float32(scale_to_meters)
    return PointCloud(
        timestamp=to_timestamp(pcl_data.getTimestamp()),
        frame_id=OPTICAL_FRAME,
        pose=IDENTITY_POSE,
        point_stride=POINT_STRIDE,
        fields=POINT_FIELDS,
        data=np.ascontiguousarray(xyz).tobytes(),
    )


def detect_point_cloud_scale(pcl_data: dai.PointCloudData) -> float:
    """
    Return the factor that converts this device's point coordinates to meters.

    We request meters from DepthAI, but some device-side PointCloud builds
    still emit millimeters, so check the actual depth magnitude once: a median
    distance beyond 50 m means the values can only be millimeters.
    """
    points = pcl_data.getPoints()
    z = np.abs(points.reshape(-1, points.shape[-1])[:, 2])
    z = z[np.isfinite(z) & (z > 0)]
    if z.size > 0 and float(np.median(z)) > 50.0:
        logging.info("Point cloud is in millimeters; converting to meters")
        return 0.001
    return 1.0


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
    # Channels with a well-known Foxglove schema (RawImage, PointCloud, ...)
    # have typed classes. The IMU uses a generic JSON channel with a
    # sensor_msgs-like schema.
    # ------------------------------------------------------------------
    rgb_channel = RawImageChannel(topic="/oak/rgb/image")
    point_cloud_channel = PointCloudChannel(topic="/oak/points")
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
    #   CAM_B + CAM_C ──> StereoDepth ──> PointCloud ──> host
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

        # Stereo pair -> depth -> point cloud, all computed on the device.
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

        point_cloud = pipeline.create(dai.node.PointCloud)
        point_cloud.initialConfig.setLengthUnit(dai.LengthUnit.METER)
        stereo.depth.link(point_cloud.inputDepth)
        point_cloud_queue = point_cloud.outputPointCloud.createOutputQueue(
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



        # --------------------------------------------------------------
        # Step 3: publish loop. tryGet() never blocks, so one loop can
        # service all three queues at their own rates.
        # --------------------------------------------------------------
        point_cloud_scale: float | None = None
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
                    rgb_channel.log(
                        RawImage(
                            timestamp=to_timestamp(frame.getTimestamp()),
                            frame_id=OPTICAL_FRAME,
                            width=width,
                            height=height,
                            encoding="bgr8",
                            step=width * 3,
                            data=bgr.tobytes(),
                        )
                    )

                pcl_data = point_cloud_queue.tryGet()
                if isinstance(pcl_data, dai.PointCloudData):
                    if point_cloud_scale is None:
                        point_cloud_scale = detect_point_cloud_scale(pcl_data)
                    message = point_cloud_to_msg(pcl_data, point_cloud_scale)
                    if message is not None:
                        point_cloud_channel.log(message)

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
