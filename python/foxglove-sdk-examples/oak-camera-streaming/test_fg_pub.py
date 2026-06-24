#!/usr/bin/env python3
"""
Publish an OAK depth point cloud to Foxglove using the Foxglove SDK.

This reuses the depth pipeline from ``test_depth_cam.py`` (a color camera plus
a stereo / neural / ToF depth source feeding a DepthAI ``RGBD`` node), but
instead of streaming through DepthAI's ``RemoteConnection`` it converts each
``PointCloudData`` packet into a ``foxglove.PointCloud`` message and logs it to
a Foxglove WebSocket server.

Topics:

- ``/oak/points``  colored point cloud (``foxglove.PointCloud``)
- ``/tf``          static optical -> upright transform (``foxglove.FrameTransforms``)

Open https://app.foxglove.dev and connect to ws://localhost:8765 (or follow the
printed link), then add a 3D panel with the display frame set to ``oak``.
"""

from __future__ import annotations

import argparse
import logging
import time
from typing import Any

import depthai as dai
import foxglove
import numpy as np
from foxglove.channels import FrameTransformsChannel, PointCloudChannel
from foxglove.messages import (
    FrameTransform,
    FrameTransforms,
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Quaternion,
    Timestamp,
    Vector3,
)

NEURAL_FPS = 8
STEREO_DEFAULT_FPS = 30
TOF_DEFAULT_FPS = 30

# DepthAI emits points in the camera optical frame (X right, Y down, Z forward).
# Publishing this rotation lets the 3D panel render the cloud upright when the
# display frame is "oak".
CAMERA_FRAME = "oak"
OPTICAL_FRAME = "oak_optical"
OPTICAL_ROTATION = Quaternion(x=-0.5, y=0.5, z=-0.5, w=0.5)

# One colored point is 16 bytes: three float32 (x, y, z) followed by a packed
# uint32 holding the bytes B, G, R, A in memory order. This is the layout the
# Foxglove 3D panel expects for an "rgba" field.
POINT_STRIDE = 16
_F32 = PackedElementFieldNumericType.Float32
_U32 = PackedElementFieldNumericType.Uint32
POINT_FIELDS = [
    PackedElementField(name="x", offset=0, type=_F32),
    PackedElementField(name="y", offset=4, type=_F32),
    PackedElementField(name="z", offset=8, type=_F32),
    PackedElementField(name="rgba", offset=12, type=_U32),
]

# Foxglove's 3D panel expects point coordinates in meters. Luxonis docs say
# PointCloudData coordinates are in the configured depth unit, millimeters by
# default. In practice, RGBD.setDepthUnits can vary across DepthAI versions, so
# the default below infers whether a packet is meter-scale or millimeter-scale.
MILLIMETERS_TO_METERS = 0.001
AUTO_MILLIMETER_Z_THRESHOLD = 50.0


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
    configured length unit and an (N, 4) uint8 array of RGBA colors. Foxglove
    expects meters, so ``point_unit`` controls or infers the scale. Invalid
    points sit at the origin, so we drop them.
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
    # Source colors are RGBA; pack as B, G, R, A so the little-endian uint32
    # decodes correctly in the Foxglove 3D panel.
    buffer[:, 12] = colors[:, 2]
    buffer[:, 13] = colors[:, 1]
    buffer[:, 14] = colors[:, 0]
    buffer[:, 15] = colors[:, 3]

    return PointCloud(
        timestamp=to_timestamp(pcl.getTimestamp()),
        frame_id=OPTICAL_FRAME,
        point_stride=POINT_STRIDE,
        fields=POINT_FIELDS,
        data=buffer.tobytes(),
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8765, help="WebSocket port")
    parser.add_argument(
        "--depth-source",
        type=str,
        default="stereo",
        choices=["stereo", "neural", "tof"],
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


def build_pipeline(
    pipeline: dai.Pipeline, depth_source: str
) -> dai.node.RGBD:
    """Build the color + depth + RGBD graph, returning the RGBD node.

    This mirrors the working pipeline in ``test_depth_cam.py``.
    """
    size = (640, 400)
    if depth_source == "neural":
        fps = NEURAL_FPS
    elif depth_source == "tof":
        fps = TOF_DEFAULT_FPS
    else:
        fps = STEREO_DEFAULT_FPS

    if depth_source == "stereo":
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
    elif depth_source == "tof":
        color = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_C, sensorFps=fps
        )
        socket, preset_mode = (
            dai.CameraBoardSocket.AUTO,
            dai.ImageFiltersPresetMode.TOF_MID_RANGE,
        )
        depth = pipeline.create(dai.node.ToF).build(socket, preset_mode)
    else:
        raise ValueError(f"Invalid depth source: {depth_source}")

    rgbd = pipeline.create(dai.node.RGBD).build(color, depth, size, fps)
    # Ask DepthAI for meter-scale output. The host-side conversion still has an
    # auto mode because some DepthAI versions/devices appear to emit the point
    # cloud in millimeters despite this setting.
    rgbd.setDepthUnits(dai.LengthUnit.METER)
    return rgbd


def main() -> None:
    args = parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")

    points_channel = PointCloudChannel(topic="/oak/points")
    tf_channel = FrameTransformsChannel(topic="/tf")

    writer = foxglove.open_mcap(args.record) if args.record else None

    server = foxglove.start_server(port=args.port)
    logging.info("Foxglove server: %s", server.app_url())

    with dai.Pipeline() as pipeline:
        rgbd = build_pipeline(pipeline, args.depth_source)
        pcl_queue = rgbd.pcl.createOutputQueue(maxSize=4, blocking=False)

        pipeline.start()
        logging.info(
            "Pipeline running with depth source %r, point unit %r — Ctrl+C to stop",
            args.depth_source,
            args.point_unit,
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
