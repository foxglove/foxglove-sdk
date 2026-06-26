#!/usr/bin/env python3
"""
Publish OAK RGBD camera data to Foxglove using the Foxglove SDK.

This reuses the depth pipeline from ``test_depth_cam.py`` (a color camera plus
a stereo / neural depth source feeding a DepthAI ``RGBD`` node), but
instead of streaming through DepthAI's ``RemoteConnection`` it converts camera,
point cloud, calibration, and IMU packets into Foxglove messages and logs them
to a Foxglove WebSocket server.

Topics:

- ``/oak/points``           colored point cloud (``foxglove.PointCloud``)
- ``/oak/rgb/image``        RGB image (``foxglove.RawImage``)
- ``/oak/rgb/calibration``  RGB camera calibration (``foxglove.CameraCalibration``)
- ``/oak/imu``              IMU samples (JSON, ``sensor_msgs.msg.ImuLike``)
- ``/tf``                   optical -> upright transform (``foxglove.FrameTransforms``)

Open https://app.foxglove.dev and connect to ws://localhost:8765 (or follow the
printed link), then add a 3D panel with the display frame set to ``oak``.
"""

from __future__ import annotations

import argparse
import json
import logging
import time
from typing import Any

import depthai as dai
import numpy as np
import foxglove
from foxglove import Channel, Schema
from foxglove.channels import (
    CameraCalibrationChannel,
    FrameTransformsChannel,
    PointCloudChannel,
    RawImageChannel,
)
from foxglove.messages import (
    CameraCalibration,
    FrameTransform,
    FrameTransforms,
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Quaternion,
    RawImage,
    Timestamp,
    Vector3,
)

NEURAL_FPS = 30
STEREO_DEFAULT_FPS = 30
IMU_HZ = 50

# DepthAI emits points in the camera optical frame (X right, Y down, Z forward).
# Publishing this rotation lets the 3D panel render the cloud upright when the
# display frame is "oak".
CAMERA_FRAME = "oak"
OPTICAL_FRAME = "oak_optical"
OPTICAL_ROTATION = Quaternion(x=-0.5, y=0.5, z=-0.5, w=0.5)

# One colored point is 16 bytes: three float32 (x, y, z) followed by separate
# uint8 red, green, blue, and alpha fields. This is the color layout the
# Foxglove 3D panel supports for foxglove.PointCloud.
POINT_STRIDE = 16
_F32 = PackedElementFieldNumericType.Float32
_U8 = PackedElementFieldNumericType.Uint8
POINT_FIELDS = [
    PackedElementField(name="x", offset=0, type=_F32),
    PackedElementField(name="y", offset=4, type=_F32),
    PackedElementField(name="z", offset=8, type=_F32),
    PackedElementField(name="red", offset=12, type=_U8),
    PackedElementField(name="green", offset=13, type=_U8),
    PackedElementField(name="blue", offset=14, type=_U8),
    PackedElementField(name="alpha", offset=15, type=_U8),
]

# Foxglove's 3D panel expects point coordinates in meters. Luxonis docs say
# PointCloudData coordinates are in the configured depth unit, millimeters by
# default. In practice, RGBD.setDepthUnits can vary across DepthAI versions, so
# the default below infers whether a packet is meter-scale or millimeter-scale.
MILLIMETERS_TO_METERS = 0.001
AUTO_MILLIMETER_Z_THRESHOLD = 50.0

# Foxglove supports a fixed set of distortion models. DepthAI's Perspective
# model is OpenCV's rational polynomial model; Foxglove uses the first 8 params.
_DISTORTION_MODELS = {
    "Perspective": ("rational_polynomial", 8),
    "Fisheye": ("kannala_brandt", 4),
}

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


def to_timestamp(td: Any) -> Timestamp:
    """Convert a DepthAI timestamp (a timedelta) to a Foxglove Timestamp."""
    try:
        total_ns = int(td.total_seconds() * 1e9)
        return Timestamp(sec=total_ns // 1_000_000_000, nsec=total_ns % 1_000_000_000)
    except Exception:
        return Timestamp.now()


def point_scale_for_unit(points: np.ndarray, point_unit: str) -> float:
    if point_unit == "meters":
        return 1.0
    if point_unit == "millimeters":
        return MILLIMETERS_TO_METERS

    median_z = float(np.median(points[:, 2])) if points.size else 0.0
    if median_z > AUTO_MILLIMETER_Z_THRESHOLD:
        return MILLIMETERS_TO_METERS
    return 1.0


def pointcloud_to_message(pcl: dai.PointCloudData, point_unit: str) -> PointCloud:
    """Convert DepthAI colored point cloud data into a Foxglove PointCloud.

    ``getPointsRGB`` returns an (N, 3) float32 array of XYZ in DepthAI's
    configured length unit and an (N, 4) uint8 array of RGB color bytes plus an
    unused fourth byte. Foxglove expects meters, so ``point_unit`` controls or
    infers the scale. Invalid points sit at the origin, so we drop them.
    """
    points, colors = pcl.getPointsRGB()
    points = np.asarray(points, dtype=np.float32).reshape(-1, 3)
    colors = np.asarray(colors, dtype=np.uint8).reshape(-1, 4)

    # Drop invalid points (depth holes are reported at z == 0).
    valid = points[:, 2] > 0.0
    points = points[valid].astype(np.float32, copy=True)
    colors = colors[valid]

    scale = point_scale_for_unit(points, point_unit)
    if scale != 1.0:
        points *= scale

    n = points.shape[0]
    buffer = np.zeros((n, POINT_STRIDE), dtype=np.uint8)
    buffer[:, 0:12] = points.view(np.uint8).reshape(n, 12)
    # Source colors are RGB plus an unused fourth byte; publish opaque alpha
    # because Foxglove uses the alpha field directly in rgba-fields mode.
    buffer[:, 12] = colors[:, 0]
    buffer[:, 13] = colors[:, 1]
    buffer[:, 14] = colors[:, 2]
    buffer[:, 15] = 255

    return PointCloud(
        timestamp=to_timestamp(pcl.getTimestamp()),
        frame_id=OPTICAL_FRAME,
        point_stride=POINT_STRIDE,
        fields=POINT_FIELDS,
        data=buffer.tobytes(),
    )


def build_camera_calibration_kwargs(
    calib: dai.CalibrationHandler,
    socket: dai.CameraBoardSocket,
    width: int,
    height: int,
    frame_id: str,
) -> dict[str, Any]:
    K = np.asarray(calib.getCameraIntrinsics(socket, width, height)).flatten().tolist()
    fx, _, cx, _, fy, cy, _, _, _ = K
    P = [
        fx,
        0.0,
        cx,
        0.0,
        0.0,
        fy,
        cy,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
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
    parser.add_argument("--port", type=int, default=8765, help="WebSocket port")
    parser.add_argument(
        "--depth-source",
        type=str,
        default="stereo",
        choices=["stereo", "neural"],
    )
    parser.add_argument(
        "--record", default="", help="Also record to an MCAP file at this path"
    )
    parser.add_argument(
        "--point-unit",
        default="auto",
        choices=["auto", "meters", "millimeters"],
        help=(
            "Unit of Luxonis PointCloudData XYZ values before publishing to "
            "Foxglove. 'auto' detects millimeter-scale packets from raw Z values."
        ),
    )
    return parser.parse_args()


def build_pipeline(pipeline: dai.Pipeline, depth_source: str) -> tuple[
    dai.node.RGBD,
    dai.Node.Output,
    dai.Node.Output,
    dai.CameraBoardSocket,
    tuple[int, int],
]:
    """Build the color + depth + RGBD graph, returning the RGBD node.

    This mirrors the working pipeline in ``test_depth_cam.py``.
    """
    size = (640, 400)
    if depth_source == "neural":
        fps = NEURAL_FPS
    else:
        fps = STEREO_DEFAULT_FPS

    if depth_source == "stereo":
        color_socket = dai.CameraBoardSocket.CAM_A
        color = pipeline.create(dai.node.Camera).build(sensorFps=fps)
        left = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_B, sensorFps=fps
        )
        right = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_C, sensorFps=fps
        )
        depth = pipeline.create(dai.node.StereoDepth)
        depth.setDefaultProfilePreset(dai.node.StereoDepth.PresetMode.DEFAULT)
        depth.setRectifyEdgeFillColor(0)
        depth.enableDistortionCorrection(True)
        left.requestOutput(size).link(depth.left)
        right.requestOutput(size).link(depth.right)
    elif depth_source == "neural":
        color_socket = dai.CameraBoardSocket.CAM_A
        color = pipeline.create(dai.node.Camera).build(sensorFps=fps)
        left = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_B, sensorFps=fps
        )
        right = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_C, sensorFps=fps
        )
        depth = pipeline.create(dai.node.NeuralDepth).build(
            left.requestOutput(size),
            right.requestOutput(size),
            dai.DeviceModelZoo.NEURAL_DEPTH_LARGE,
        )
    else:
        raise ValueError(f"Invalid depth source: {depth_source}")

    rgbd = pipeline.create(dai.node.RGBD).build(color, depth, size, fps)
    color_out = color.requestOutput(size, type=dai.ImgFrame.Type.NV12, fps=fps)
    # Ask DepthAI for meter-scale output. The host-side conversion still has an
    # auto mode because some DepthAI versions/devices appear to emit the point
    # cloud in millimeters despite this setting.
    rgbd.setDepthUnits(dai.LengthUnit.METER)

    imu = pipeline.create(dai.node.IMU)
    imu.enableIMUSensor(dai.IMUSensor.ACCELEROMETER_UNCALIBRATED, IMU_HZ)
    imu.enableIMUSensor(dai.IMUSensor.GYROSCOPE_UNCALIBRATED, IMU_HZ)
    imu.setBatchReportThreshold(5)
    imu.setMaxBatchReports(20)

    return rgbd, color_out, imu.out, color_socket, size


def main() -> None:
    args = parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")

    points_channel = PointCloudChannel(topic="/oak/points")
    rgb_channel = RawImageChannel(topic="/oak/rgb/image")
    cal_channel = CameraCalibrationChannel(topic="/oak/rgb/calibration")
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

    writer = foxglove.open_mcap(args.record) if args.record else None

    server = foxglove.start_server(port=args.port)
    logging.info("Foxglove server: %s", server.app_url())

    with dai.Pipeline() as pipeline:
        rgbd, color_out, imu_out, color_socket, color_size = build_pipeline(
            pipeline, args.depth_source
        )
        pcl_queue = rgbd.pcl.createOutputQueue(maxSize=4, blocking=False)
        rgb_queue = color_out.createOutputQueue(maxSize=2, blocking=False)
        imu_queue = imu_out.createOutputQueue(maxSize=50, blocking=False)

        pipeline.start()
        logging.info(
            "Pipeline running with depth source %r, point unit %r, IMU %dHz — Ctrl+C to stop",
            args.depth_source,
            args.point_unit,
            IMU_HZ,
        )

        device = pipeline.getDefaultDevice()
        rgb_calibration_kwargs = build_camera_calibration_kwargs(
            device.readCalibration(),
            color_socket,
            color_size[0],
            color_size[1],
            OPTICAL_FRAME,
        )

        try:
            while pipeline.isRunning():
                # Keep republishing the optical-frame transform so the 3D panel
                # always has a recent one to orient the cloud.
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

                pcl = pcl_queue.tryGet()
                if isinstance(pcl, dai.PointCloudData):
                    points_channel.log(pointcloud_to_message(pcl, args.point_unit))

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
                    cal_channel.log(
                        CameraCalibration(timestamp=stamp, **rgb_calibration_kwargs)
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

                time.sleep(0.001)
        except KeyboardInterrupt:
            logging.info("Stopping…")
        finally:
            pipeline.stop()

    if writer is not None:
        writer.close()
        logging.info("MCAP written to %s", args.record)


if __name__ == "__main__":
    main()
