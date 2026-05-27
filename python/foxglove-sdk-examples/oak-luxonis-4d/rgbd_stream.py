#!/usr/bin/env python3
"""
RGBD Point Cloud Visualization → Foxglove (DepthAI v3 / OAK-4).

Based on the official Luxonis example:
  https://docs.luxonis.com/software-v3/depthai/examples/rgbd/rgbd/

Streams the device-side RGBD-aligned, colored point cloud from a Luxonis OAK
device to Foxglove via the Foxglove SDK WebSocket server.

Highlights
----------
- Uses ``dai.node.RGBD`` (depth aligned to color) so points share the color
  optical frame.
- Requests ``DepthUnit.METER`` from the RGBD node so XYZ is published in
  Foxglove's native meter-based scene without host-side scaling. A safety
  heuristic still detects mm-scale output if the device ignores the request.
- Publishes static TF from the device calibration (matches depthai-ros
  ``TFPublisher``); TF helpers are reused from ``main.py``.
- Foxglove ``PointCloud`` is packed as XYZ float32 + four separate Uint8
  ``red`` / ``green`` / ``blue`` / ``alpha`` fields (RGBA byte order), which is
  the "RGBA (separate fields)" mode documented for ``foxglove.PointCloud``:
  https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors
"""

from __future__ import annotations

import argparse
import logging
import struct
import time
from dataclasses import dataclass
from typing import Any

import depthai as dai
import foxglove
import numpy as np
from foxglove.channels import FrameTransformsChannel, PointCloudChannel
from foxglove.messages import (
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Pose,
    Quaternion,
    Timestamp,
    Vector3,
)

from main import (
    TfTransformSnapshot,
    build_tf_snapshots_from_calib,
    dai_ts_to_foxglove,
    log_tf_snapshots,
)

STEREO_DEFAULT_FPS = 30
NEURAL_FPS = 8
TOF_DEFAULT_FPS = 30
DEFAULT_SIZE: tuple[int, int] = (640, 400)

# Color convention for `foxglove.PointCloud` (NOT ROS `sensor_msgs/PointCloud2`):
# four separate Uint8 fields named red / green / blue / alpha, in RGBA byte order.
# Foxglove Studio's 3D panel exposes this as the "RGBA (separate fields)" color
# mode for `foxglove.PointCloud` topics. See:
#   https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors
# (Packed Uint32 `rgba` in BGRA order is the convention for ROS PointCloud2,
# i.e. the "BGRA (packed)" mode, and not the documented mode for this schema.)
POINT_CLOUD_FIELDS: list[PackedElementField] = [
    PackedElementField(name="x", offset=0, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="y", offset=4, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="z", offset=8, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="red", offset=12, type=PackedElementFieldNumericType.Uint8),
    PackedElementField(name="green", offset=13, type=PackedElementFieldNumericType.Uint8),
    PackedElementField(name="blue", offset=14, type=PackedElementFieldNumericType.Uint8),
    PackedElementField(name="alpha", offset=15, type=PackedElementFieldNumericType.Uint8),
]
POINT_STRUCT = struct.Struct("<fffBBBB")  # x, y, z, R, G, B, A
POINT_STRIDE = POINT_STRUCT.size  # 16 bytes

POINT_CLOUD_POSE = Pose(
    position=Vector3(x=0.0, y=0.0, z=0.0),
    orientation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
)

# DepthAI's PointCloud path defaults to millimeters on some builds. If the
# RGBD node ignores DepthUnit.METER, host-detect mm by checking median |Z|.
# Median |Z| > 50.0 is impossibly far for stereo / ToF if we're in meters.
MM_HEURISTIC_THRESHOLD_M = 50.0

UNIT_OVERRIDE_SCALES: dict[str, float] = {
    "auto": 0.0,  # sentinel: detect on first cloud
    "meter": 1.0,
    "mm": 0.001,
    "millimeter": 0.001,
    "cm": 0.01,
    "centimeter": 0.01,
    "inch": 0.0254,
    "foot": 0.3048,
}


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument(
        "--depth-source",
        choices=("stereo", "neural", "tof"),
        default="stereo",
        help="Depth source: stereo (default), neural, or tof",
    )
    p.add_argument(
        "--fps",
        type=int,
        default=None,
        help="Override pipeline FPS (default depends on --depth-source)",
    )
    p.add_argument("--width", type=int, default=DEFAULT_SIZE[0], help="Output width")
    p.add_argument("--height", type=int, default=DEFAULT_SIZE[1], help="Output height")
    p.add_argument(
        "--frame-id",
        default=None,
        help="Override point cloud frame_id (default: {tf-prefix}_rgb_camera_optical_frame)",
    )
    p.add_argument("--tf-prefix", default="oak", help="TF frame prefix (depthai-ros style)")
    p.add_argument("--tf-base-frame", default="oak", help="TF base frame name")
    p.add_argument(
        "--ir-laser-intensity",
        type=float,
        default=0.7,
        help="IR laser dot projector intensity (0..1); matches the C++ rerun example",
    )
    p.add_argument("--record", default="", help="Optional MCAP output path")
    p.add_argument(
        "--unit-override",
        choices=tuple(UNIT_OVERRIDE_SCALES.keys()),
        default="auto",
        help=(
            "Force interpretation of the device-reported point coordinates. "
            "'auto' (default) trusts DepthAI's METER request and falls back to "
            "an mm-detection heuristic. Use 'mm' if points still look ~1000x too "
            "far, 'meter' to disable scaling, or 'cm' / 'inch' / 'foot' if needed."
        ),
    )
    p.add_argument(
        "--log-every-n",
        type=int,
        default=0,
        help="Log per-cloud diagnostic stats every N clouds (0 disables; first cloud is always logged at INFO)",
    )
    return p.parse_args()


def fps_for_source(source: str, override: int | None) -> int:
    if override is not None:
        return int(override)
    if source == "neural":
        return NEURAL_FPS
    if source == "tof":
        return TOF_DEFAULT_FPS
    return STEREO_DEFAULT_FPS


def read_device_tf_snapshots(
    tf_prefix: str, tf_base_frame: str
) -> list[TfTransformSnapshot]:
    """Open a dai.Device once to read calibration and build the TF tree."""
    try:
        with dai.Device() as device:
            calib = device.readCalibration2()
            return build_tf_snapshots_from_calib(
                calib, tf_prefix=tf_prefix, tf_base_frame=tf_base_frame
            )
    except Exception as ex:
        logging.warning("Could not read device calibration for TF: %s", ex)
        return []


def _try_set_depth_units_meter(rgbd_node: Any) -> bool:
    """Try the various DepthAI APIs that request METER output. Returns True on success."""
    cfg_ctrl = getattr(getattr(dai, "StereoDepthConfig", None), "AlgorithmControl", None)
    unit_meter = getattr(getattr(cfg_ctrl, "DepthUnit", None), "METER", None)
    if unit_meter is None:
        return False
    for fn_name in ("setDepthUnits", "setDepthUnit"):
        fn = getattr(rgbd_node, fn_name, None)
        if callable(fn):
            try:
                fn(unit_meter)
                return True
            except Exception:
                continue
    return False


def _axis_stats(a: np.ndarray) -> dict[str, float]:
    """Min/median/max + abs median for one axis. Cheap; used in diagnostic logging."""
    if a.size == 0:
        return {"min": float("nan"), "max": float("nan"), "median": float("nan"), "abs_median": float("nan")}
    return {
        "min": float(np.min(a)),
        "max": float(np.max(a)),
        "median": float(np.median(a)),
        "abs_median": float(np.median(np.abs(a))),
    }


def _detect_meter_scale(z_values: np.ndarray) -> tuple[float, float]:
    """
    Heuristic mm-vs-meter detector based on positive Z magnitudes.

    Returns ``(scale_to_meters, median_abs_positive_z)``.
    """
    pos = np.abs(z_values[z_values > 0])
    if pos.size == 0:
        return 1.0, 0.0
    median_z = float(np.median(pos))
    return (0.001 if median_z > MM_HEURISTIC_THRESHOLD_M else 1.0), median_z


@dataclass
class PclDiagnostics:
    """Persistent state for first-cloud / periodic logging and scale lock-in."""

    user_override: str  # one of UNIT_OVERRIDE_SCALES keys
    locked_scale: float | None = None  # final scale used after first cloud
    first_logged: bool = False
    nan_points: int = 0
    inf_points: int = 0


def build_pcl_message(
    pcl_data: Any,
    ts: Timestamp,
    frame_id: str,
    diag: PclDiagnostics,
    *,
    verbose: bool = False,
) -> PointCloud | None:
    """Convert ``dai.PointCloudData`` to a Foxglove ``PointCloud`` (XYZ + RGBA)."""

    points: np.ndarray
    colors: np.ndarray | None = None
    rgb_path = "(none)"
    try:
        result = pcl_data.getPointsRGB()
        rgb_path = "getPointsRGB()"
    except Exception:
        result = None
    if isinstance(result, tuple) and len(result) == 2:
        points, colors = result
    else:
        try:
            points = pcl_data.getPoints()
            rgb_path = "getPoints() (no colors)"
        except Exception:
            return None

    if not isinstance(points, np.ndarray) or points.size == 0:
        return None

    raw_shape = points.shape
    raw_dtype = points.dtype
    pts = points.reshape(-1, points.shape[-1])[:, :3].astype(np.float32, copy=False)
    if pts.size == 0:
        return None

    finite = np.isfinite(pts).all(axis=1)
    n_total = int(pts.shape[0])
    n_finite = int(finite.sum())
    if n_finite == 0:
        if not diag.first_logged:
            logging.warning(
                "PointCloud first cloud: %d points but none are finite (NaN/Inf). raw_shape=%s dtype=%s rgb=%s",
                n_total, raw_shape, raw_dtype, rgb_path,
            )
            diag.first_logged = True
        return None

    if isinstance(colors, np.ndarray) and colors.size:
        col = colors.reshape(-1, colors.shape[-1])
        if col.shape[0] < pts.shape[0]:
            colors = None
        else:
            colors = col[: pts.shape[0]].astype(np.uint8, copy=False)

    pts_all = pts.copy()  # before filtering, for diagnostics
    pts = pts[finite]
    if colors is not None:
        colors = colors[finite]

    # Decide scale.
    override = diag.user_override
    _detected_scale, detected_median_z = _detect_meter_scale(pts[:, 2])
    if override != "auto":
        scale = UNIT_OVERRIDE_SCALES[override]
        scale_source = f"--unit-override={override}"
        diag.locked_scale = scale
    else:
        # Always re-detect (we don't trust setDepthUnits blindly). Lock in after first cloud
        # so the publish rate doesn't get warning spam every frame.
        if diag.locked_scale is None:
            scale = _detected_scale
            diag.locked_scale = scale
        else:
            scale = diag.locked_scale
        scale_source = "auto-detect"

    if not diag.first_logged or verbose:
        x_stats = _axis_stats(pts_all[:, 0])
        y_stats = _axis_stats(pts_all[:, 1])
        z_stats = _axis_stats(pts_all[:, 2])
        n_non_finite = n_total - n_finite
        log_fn = logging.info if not diag.first_logged else logging.debug
        log_fn(
            "PointCloud first cloud: shape=%s dtype=%s rgb=%s; "
            "points total=%d finite=%d non_finite=%d; "
            "X[min/med/max]=%.3f/%.3f/%.3f Y=%.3f/%.3f/%.3f Z=%.3f/%.3f/%.3f; "
            "abs_median Z (positive)=%.4f -> scale=%.5g (%s) -> publishing in meters",
            raw_shape, raw_dtype, rgb_path,
            n_total, n_finite, n_non_finite,
            x_stats["min"], x_stats["median"], x_stats["max"],
            y_stats["min"], y_stats["median"], y_stats["max"],
            z_stats["min"], z_stats["median"], z_stats["max"],
            detected_median_z, scale, scale_source,
        )
        if colors is not None and colors.shape[1] >= 3 and not diag.first_logged:
            log_fn(
                "PointCloud first cloud colors: shape=%s dtype=%s sample[0]=%s",
                colors.shape, colors.dtype, colors[0].tolist() if colors.size else "[]",
            )
        diag.first_logged = True

    if scale != 1.0:
        pts = pts * np.float32(scale)

    n = int(pts.shape[0])
    # Vectorized pack: 16-byte structured record matching POINT_STRUCT ('<fffBBBB').
    # Byte order on the wire: x(0..3) y(4..7) z(8..11) R(12) G(13) B(14) A(15)
    # — matches POINT_CLOUD_FIELDS ("red","green","blue","alpha" at offsets 12..15).
    record_dtype = np.dtype(
        [
            ("x", "<f4"),
            ("y", "<f4"),
            ("z", "<f4"),
            ("r", "u1"),
            ("g", "u1"),
            ("b", "u1"),
            ("a", "u1"),
        ]
    )
    assert record_dtype.itemsize == POINT_STRIDE
    rec = np.empty(n, dtype=record_dtype)
    pts_c = np.ascontiguousarray(pts, dtype=np.float32)
    rec["x"] = pts_c[:, 0]
    rec["y"] = pts_c[:, 1]
    rec["z"] = pts_c[:, 2]
    if colors is not None and colors.shape[1] >= 3:
        # DepthAI `getPointsRGB()` returns RGBA (alpha may be absent on some builds).
        rec["r"] = colors[:, 0]
        rec["g"] = colors[:, 1]
        rec["b"] = colors[:, 2]
        rec["a"] = colors[:, 3] if colors.shape[1] >= 4 else 255
    else:
        rec["r"] = 255
        rec["g"] = 255
        rec["b"] = 255
        rec["a"] = 255

    return PointCloud(
        timestamp=ts,
        frame_id=frame_id,
        pose=POINT_CLOUD_POSE,
        point_stride=POINT_STRIDE,
        fields=POINT_CLOUD_FIELDS,
        data=rec.tobytes(),
    )


def build_rgbd_stereo(
    pipeline: dai.Pipeline, size: tuple[int, int], fps: int
) -> Any:
    """
    Stereo RGBD path that mirrors the Rerun variant of the Luxonis example.

    Depth is aligned to color and the RGBD node consumes the aligned depth and
    the color stream; this is the configuration the example uses to set
    ``DepthUnit.METER``.
    """
    color = pipeline.create(dai.node.Camera).build()
    left = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_B)
    right = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_C)
    stereo = pipeline.create(dai.node.StereoDepth)
    rgbd = pipeline.create(dai.node.RGBD).build()

    stereo.setDefaultProfilePreset(dai.node.StereoDepth.PresetMode.DEFAULT)
    stereo.setRectifyEdgeFillColor(0)
    stereo.enableDistortionCorrection(True)
    try:
        stereo.initialConfig.postProcessing.thresholdFilter.maxRange = 10000
    except Exception:
        pass

    left.requestOutput(size).link(stereo.left)
    right.requestOutput(size).link(stereo.right)

    platform = pipeline.getDefaultDevice().getPlatform()
    if platform == dai.Platform.RVC4:
        out = color.requestOutput(
            size,
            dai.ImgFrame.Type.RGB888i,
            enableUndistortion=True,
        )
        align = pipeline.create(dai.node.ImageAlign)
        stereo.depth.link(align.input)
        out.link(align.inputAlignTo)
        align.outputAligned.link(rgbd.inDepth)
    else:
        out = color.requestOutput(
            size,
            dai.ImgFrame.Type.RGB888i,
            dai.ImgResizeMode.CROP,
            fps,
            True,
        )
        stereo.depth.link(rgbd.inDepth)
        out.link(stereo.inputAlignTo)
    out.link(rgbd.inColor)
    return rgbd


def build_rgbd_unified(
    pipeline: dai.Pipeline, depth_source: str, size: tuple[int, int], fps: int
) -> Any:
    """Unified RGBD.build() path used by the OAK Visualizer variant of the example."""
    color: Any
    depth_node: Any
    if depth_source == "neural":
        color = pipeline.create(dai.node.Camera).build(sensorFps=fps)
        left = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_B, sensorFps=fps
        )
        right = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_C, sensorFps=fps
        )
        depth_node = pipeline.create(dai.node.NeuralDepth).build(
            left.requestOutput(size),
            right.requestOutput(size),
            dai.DeviceModelZoo.NEURAL_DEPTH_LARGE,
        )
    elif depth_source == "tof":
        color = pipeline.create(dai.node.Camera).build(
            dai.CameraBoardSocket.CAM_C, sensorFps=fps
        )
        depth_node = pipeline.create(dai.node.ToF).build(
            dai.CameraBoardSocket.AUTO,
            dai.ImageFiltersPresetMode.TOF_MID_RANGE,
        )
    else:
        raise ValueError(f"Unsupported depth source for unified path: {depth_source}")

    # DepthAI v3 ships a (color, depth_source, size, fps) overload of RGBD.build at runtime
    # (see Luxonis docs), but the typed bindings only expose the no-arg / autocreate variants.
    rgbd: Any = pipeline.create(dai.node.RGBD)
    return rgbd.build(color, depth_node, size, fps)


def main() -> None:
    args = parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    foxglove.set_log_level(logging.INFO)

    size = (int(args.width), int(args.height))
    fps = fps_for_source(args.depth_source, args.fps)
    point_cloud_frame_id = (
        args.frame_id
        if args.frame_id
        else f"{args.tf_prefix}_rgb_camera_optical_frame"
    )

    tf_snapshots = read_device_tf_snapshots(args.tf_prefix, args.tf_base_frame)
    if tf_snapshots:
        logging.info("Loaded %d static TF transforms from device calibration", len(tf_snapshots))
        cloud_frame_in_tf = any(
            s.parent_frame_id == point_cloud_frame_id or s.child_frame_id == point_cloud_frame_id
            for s in tf_snapshots
        )
        for s in tf_snapshots:
            logging.info(
                "  TF %s -> %s  t=(%.4f, %.4f, %.4f) q=(%.4f, %.4f, %.4f, %.4f)",
                s.parent_frame_id, s.child_frame_id,
                s.tx, s.ty, s.tz, s.qx, s.qy, s.qz, s.qw,
            )
        if not cloud_frame_in_tf:
            logging.warning(
                "Point cloud frame_id=%s is NOT in the static TF tree above. "
                "Foxglove may place the cloud at the origin of an unrelated frame.",
                point_cloud_frame_id,
            )
    else:
        logging.warning(
            "No TF snapshots loaded; the point cloud will publish but the TF tree will be empty. "
            "In Foxglove's 3D panel, set 'Frame -> Display frame' to %s to view it.",
            point_cloud_frame_id,
        )

    pcl_channel = PointCloudChannel(topic="/oak/depth/points")
    tf_channel = FrameTransformsChannel(topic="/tf")
    tf_static_channel = FrameTransformsChannel(topic="/tf_static")

    writer = foxglove.open_mcap(args.record) if args.record else None
    server = foxglove.start_server()
    logging.info(
        "Foxglove server: %s (depth source: %s, %dx%d @ %d fps, frame_id=%s)",
        server.app_url(),
        args.depth_source,
        size[0],
        size[1],
        fps,
        point_cloud_frame_id,
    )

    if tf_snapshots:
        log_tf_snapshots(
            tf_channel,
            tf_static_channel,
            tf_snapshots,
            Timestamp.now(),
            log_static=True,
            log_tf=True,
        )

    with dai.Pipeline() as pipeline:
        if args.depth_source == "stereo":
            rgbd = build_rgbd_stereo(pipeline, size, fps)
        else:
            rgbd = build_rgbd_unified(pipeline, args.depth_source, size, fps)

        meter_units_set = _try_set_depth_units_meter(rgbd)
        if meter_units_set:
            logging.info(
                "RGBD: setDepthUnits(METER) succeeded. NOTE: succeeded != verified — "
                "first cloud is still range-checked on host (see PointCloud first cloud log)"
            )
        else:
            logging.warning(
                "RGBD: could not request METER units; relying on auto-detect / --unit-override"
            )

        pcl_queue = rgbd.pcl.createOutputQueue(maxSize=2, blocking=False)

        pipeline.start()
        try:
            device = pipeline.getDefaultDevice()
            if device is not None and hasattr(device, "setIrLaserDotProjectorIntensity"):
                try:
                    device.setIrLaserDotProjectorIntensity(float(args.ir_laser_intensity))
                except Exception as ex:
                    logging.debug("setIrLaserDotProjectorIntensity not applied: %s", ex)
        except Exception:
            pass

        logging.info("RGBD pipeline running — Ctrl+C to stop")

        diag = PclDiagnostics(user_override=args.unit_override)
        log_every_n = max(0, int(args.log_every_n))
        published = 0
        try:
            while pipeline.isRunning():
                pcl_data = pcl_queue.tryGet()
                if pcl_data is None:
                    time.sleep(0.005)
                    continue
                ts = dai_ts_to_foxglove(pcl_data) or Timestamp.now()
                verbose_now = log_every_n > 0 and (published % log_every_n == 0) and published > 0
                msg = build_pcl_message(
                    pcl_data,
                    ts,
                    point_cloud_frame_id,
                    diag,
                    verbose=verbose_now,
                )
                if msg is None:
                    continue
                pcl_channel.log(msg)
                if tf_snapshots:
                    log_tf_snapshots(
                        tf_channel,
                        tf_static_channel,
                        tf_snapshots,
                        ts,
                        log_static=False,
                        log_tf=True,
                    )
                published += 1
                if published % 30 == 0:
                    logging.info(
                        "Published %d point clouds (locked scale=%.5g, override=%s)",
                        published,
                        diag.locked_scale if diag.locked_scale is not None else float("nan"),
                        diag.user_override,
                    )
        except KeyboardInterrupt:
            logging.info("Stopping…")
        finally:
            pipeline.stop()

    if writer is not None:
        writer.close()
        logging.info("MCAP closed: %s", args.record)


if __name__ == "__main__":
    main()
