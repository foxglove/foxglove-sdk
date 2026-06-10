"""Luxonis OAK-4 -> Foxglove streamer for the reBotArm demo.

Self-contained library module that owns a DepthAI v3 pipeline (color + stereo
+ RGBD) and publishes its outputs to a Foxglove WebSocket server in a
background worker thread. The reBotArm demo uses it to overlay a live colored
point cloud (and optionally raw RGB / depth) on top of the URDF visualization.

Design goals
------------
- **One library file, one class**: ``OakStreamer`` owns everything. ``main.py``
  only does ``OakStreamer(...).start()`` and ``stop()``.
- **Graceful degradation**: if ``depthai`` is not installed or no device is
  attached, the streamer logs a warning and stops; the rest of the demo
  (homing, oscillation, URDF) keeps running.
- **TF compatible with the URDF**: published transforms are rooted at the
  ``tf_base_frame`` argument (default ``"oak"``), which is also the URDF link
  the arm mounts the camera on, so the device's calibration tree
  (``oak_rgb_camera_frame`` -> ``oak_rgb_camera_optical_frame`` etc.) hangs
  directly off the wrist-mounted ``oak`` link with no extra glue.

Code provenance
---------------
The TF, calibration, point-cloud, and RGBD-pipeline helpers were adapted
from the sibling [``oak-luxonis-4d``](../oak-luxonis-4d/main.py) and
[``rgbd_stream.py``](../oak-luxonis-4d/rgbd_stream.py) examples (DepthAI v3
reference + Luxonis depthai-ros parity). Kept here as a single module so
the demo has no cross-example import edges.
"""
from __future__ import annotations

import logging
import struct
import threading
import time
from dataclasses import dataclass, field
from typing import Any, Optional

import numpy as np

try:
    import depthai as dai
    _DEPTHAI_AVAILABLE = True
    _DEPTHAI_IMPORT_ERROR: Optional[BaseException] = None
except Exception as _ex:  # pragma: no cover - missing-device / missing-package path
    dai = None  # type: ignore[assignment]
    _DEPTHAI_AVAILABLE = False
    _DEPTHAI_IMPORT_ERROR = _ex

from foxglove import Channel, Schema
from foxglove.channels import (
    CompressedVideoChannel,
    FrameTransformsChannel,
    PointCloudChannel,
)
from foxglove.messages import (
    CompressedVideo,
    FrameTransform,
    FrameTransforms,
    PackedElementField,
    PackedElementFieldNumericType,
    PointCloud,
    Pose,
    Quaternion,
    Timestamp,
    Vector3,
)


__all__ = [
    "OakStreamer",
    "OakStreamerConfig",
    "TfTransformSnapshot",
    "is_depthai_available",
]


# --------------------------------------------------------------------------- #
# Constants                                                                   #
# --------------------------------------------------------------------------- #

STEREO_DEFAULT_FPS = 30
NEURAL_FPS = 8
TOF_DEFAULT_FPS = 30
DEFAULT_SIZE: tuple[int, int] = (640, 400)

# DepthAI's PointCloud path defaults to millimeters on some builds. If the
# RGBD node ignores DepthUnit.METER, host-detect mm by checking median |Z|:
# median |Z| > 50.0 is impossibly far for stereo / ToF if we're in meters.
MM_HEURISTIC_THRESHOLD_M = 50.0

# Color convention for `foxglove.PointCloud` (NOT ROS `sensor_msgs/PointCloud2`):
# four separate Uint8 fields named red / green / blue / alpha, in RGBA byte order.
# Foxglove Studio's 3D panel exposes this as the "RGBA (separate fields)" color
# mode for `foxglove.PointCloud` topics; see
#   https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors
POINT_CLOUD_FIELDS: list[PackedElementField] = [
    PackedElementField(name="x", offset=0, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="y", offset=4, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="z", offset=8, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="red", offset=12, type=PackedElementFieldNumericType.Uint8),
    PackedElementField(name="green", offset=13, type=PackedElementFieldNumericType.Uint8),
    PackedElementField(name="blue", offset=14, type=PackedElementFieldNumericType.Uint8),
    PackedElementField(name="alpha", offset=15, type=PackedElementFieldNumericType.Uint8),
]
_POINT_RECORD_DTYPE = np.dtype(
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
POINT_STRUCT = struct.Struct("<fffBBBB")
POINT_STRIDE = POINT_STRUCT.size  # 16 bytes

POINT_CLOUD_POSE = Pose(
    position=Vector3(x=0.0, y=0.0, z=0.0),
    orientation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
)

# Fixed camera_frame -> camera_optical_frame rotation (RDF optical),
# from depthai-ros `TFPublisher`.
Q_OPTICAL_FROM_CAMERA_FRAME: tuple[float, float, float, float] = (-0.5, 0.5, -0.5, 0.5)


def is_depthai_available() -> bool:
    """Return True iff ``import depthai`` succeeded at module load time."""
    return _DEPTHAI_AVAILABLE


# --------------------------------------------------------------------------- #
# TF helpers (parity with depthai-ros `TFPublisher`)                          #
# --------------------------------------------------------------------------- #

@dataclass(frozen=True)
class TfTransformSnapshot:
    """Static TF derived from Luxonis ``getCameraExtrinsics`` / ``getImuToCameraExtrinsics``.

    Identical layout to the one in ``oak-luxonis-4d/main.py`` so the resulting
    /tf tree matches depthai-ros 1:1.
    """

    parent_frame_id: str
    child_frame_id: str
    tx: float
    ty: float
    tz: float
    qx: float
    qy: float
    qz: float
    qw: float

    def to_msg(self, ts: Timestamp | None = None) -> FrameTransform:
        return FrameTransform(
            timestamp=ts,
            parent_frame_id=self.parent_frame_id,
            child_frame_id=self.child_frame_id,
            translation=Vector3(x=self.tx, y=self.ty, z=self.tz),
            rotation=Quaternion(x=self.qx, y=self.qy, z=self.qz, w=self.qw),
        )


def _rotation_matrix_to_quaternion(R: np.ndarray) -> tuple[float, float, float, float]:
    R = np.asarray(R, dtype=np.float64).reshape(3, 3)
    tr = float(np.trace(R))
    if tr > 0.0:
        S = np.sqrt(tr + 1.0) * 2.0
        w = 0.25 * S
        x = (R[2, 1] - R[1, 2]) / S
        y = (R[0, 2] - R[2, 0]) / S
        z = (R[1, 0] - R[0, 1]) / S
    elif R[0, 0] > R[1, 1] and R[0, 0] > R[2, 2]:
        S = np.sqrt(1.0 + R[0, 0] - R[1, 1] - R[2, 2]) * 2.0
        w = (R[2, 1] - R[1, 2]) / S
        x = 0.25 * S
        y = (R[0, 1] + R[1, 0]) / S
        z = (R[0, 2] + R[2, 0]) / S
    elif R[1, 1] > R[2, 2]:
        S = np.sqrt(1.0 + R[1, 1] - R[0, 0] - R[2, 2]) * 2.0
        w = (R[0, 2] - R[2, 0]) / S
        x = (R[0, 1] + R[1, 0]) / S
        y = 0.25 * S
        z = (R[1, 2] + R[2, 1]) / S
    else:
        S = np.sqrt(1.0 + R[2, 2] - R[0, 0] - R[1, 1]) * 2.0
        w = (R[1, 0] - R[0, 1]) / S
        x = (R[0, 2] + R[2, 0]) / S
        y = (R[1, 2] + R[2, 1]) / S
        z = 0.25 * S
    n = (x * x + y * y + z * z + w * w) ** 0.5
    if n < 1e-12:
        return 0.0, 0.0, 0.0, 1.0
    return float(x / n), float(y / n), float(z / n), float(w / n)


def _quaternion_to_rotmat(qx: float, qy: float, qz: float, qw: float) -> np.ndarray:
    n = (qx * qx + qy * qy + qz * qz + qw * qw) ** 0.5
    if n < 1e-12:
        return np.eye(3, dtype=np.float64)
    x, y, z, w = qx / n, qy / n, qz / n, qw / n
    xx, yy, zz = x * x, y * y, z * z
    xy, xz, yz = x * y, x * z, y * z
    wx, wy, wz = w * x, w * y, w * z
    return np.array(
        [
            [1 - 2 * (yy + zz), 2 * (xy - wz), 2 * (xz + wy)],
            [2 * (xy + wz), 1 - 2 * (xx + zz), 2 * (yz - wx)],
            [2 * (xz - wy), 2 * (yz + wx), 1 - 2 * (xx + yy)],
        ],
        dtype=np.float64,
    )


def _lux_extrinsic_rotation_to_ros_camera_frame(R_lux: np.ndarray) -> np.ndarray:
    """Same basis change as ``depthai_bridge::TFPublisher::quatFromRotM``."""
    q_spin = Q_OPTICAL_FROM_CAMERA_FRAME
    R_spin = _quaternion_to_rotmat(*q_spin)
    R = np.asarray(R_lux, dtype=np.float64).reshape(3, 3)
    return R_spin @ R @ R_spin.T


def _translation_lux_optical_to_ros_rdf(translation: Any) -> tuple[float, float, float]:
    """Match ``depthai_bridge::TFPublisher::transFromExtr`` (cm -> m, axis remap)."""
    t = np.asarray(translation, dtype=np.float64).reshape(-1)
    if t.size < 3:
        return 0.0, 0.0, 0.0
    x, y, z = float(t[0]), float(t[1]), float(t[2])
    return z / 100.0, x / -100.0, y / -100.0


def _camera_board_socket_name(sock: Any) -> str:
    if not _DEPTHAI_AVAILABLE:
        return f"cam_{int(sock)}"
    m = {
        dai.CameraBoardSocket.CAM_A: "rgb",
        dai.CameraBoardSocket.CAM_B: "left",
        dai.CameraBoardSocket.CAM_C: "right",
        dai.CameraBoardSocket.CAM_D: "left_back",
        dai.CameraBoardSocket.CAM_E: "right_back",
    }
    try:
        return m[sock]
    except KeyError:
        return f"cam_{int(sock)}"


def _frame_camera(prefix: str, socket_name: str) -> str:
    return f"{prefix}_{socket_name}_camera_frame"


def _frame_optical(prefix: str, socket_name: str) -> str:
    return f"{prefix}_{socket_name}_camera_optical_frame"


def _tf_rigid(parent: str, child: str, R: np.ndarray, t_xyz: tuple[float, float, float]) -> TfTransformSnapshot:
    qx, qy, qz, qw = _rotation_matrix_to_quaternion(np.asarray(R, dtype=np.float64).reshape(3, 3))
    return TfTransformSnapshot(parent, child, t_xyz[0], t_xyz[1], t_xyz[2], qx, qy, qz, qw)


def _tf_quat(parent: str, child: str, q: tuple[float, float, float, float], t_xyz: tuple[float, float, float]) -> TfTransformSnapshot:
    qx, qy, qz, qw = q
    return TfTransformSnapshot(parent, child, t_xyz[0], t_xyz[1], t_xyz[2], qx, qy, qz, qw)


def _eeprom_as_dict(calib: Any) -> dict[str, Any]:
    import json
    raw = calib.eepromToJson()
    if isinstance(raw, str):
        return json.loads(raw)
    if isinstance(raw, dict):
        return raw
    return dict(raw)


def build_tf_snapshots_from_calib(
    calib: Any, *, tf_prefix: str, tf_base_frame: str
) -> list[TfTransformSnapshot]:
    """Build a ``depthai_bridge::TFPublisher``-compatible static TF tree.

    Returns ``{prefix}_{rgb|left|right}_camera_frame`` rigid transforms wired
    by extrinsics, each linked to ``{prefix}_*_camera_optical_frame`` via the
    standard optical rotation, plus ``{prefix}_imu_frame`` via the IMU<->camera
    extrinsics and the RDF-style quaternion the ROS driver uses.
    """
    if not _DEPTHAI_AVAILABLE:
        return []

    out: list[TfTransformSnapshot] = []

    def add_optical_joint(socket_name: str) -> None:
        parent = _frame_camera(tf_prefix, socket_name)
        child = _frame_optical(tf_prefix, socket_name)
        out.append(_tf_quat(parent, child, Q_OPTICAL_FROM_CAMERA_FRAME, (0.0, 0.0, 0.0)))

    data: dict[str, Any] = {}
    try:
        data = _eeprom_as_dict(calib)
        cam_data = data.get("cameraData")
    except Exception as ex:
        logging.warning("[oak] eepromToJson failed; using socket fallback for TF: %s", ex)
        cam_data = None

    used_optical: set[str] = set()

    if isinstance(cam_data, list) and cam_data:
        for entry in cam_data:
            if not isinstance(entry, (list, tuple)) or len(entry) < 2:
                continue
            try:
                curr_cam = dai.CameraBoardSocket(int(entry[0]))
            except Exception:
                continue
            info = entry[1]
            if not isinstance(info, dict):
                continue
            extr = info.get("extrinsics")
            if not isinstance(extr, dict):
                continue
            to_sock = extr.get("toCameraSocket", -1)
            sock_name = _camera_board_socket_name(curr_cam)
            child_frame = _frame_camera(tf_prefix, sock_name)
            try:
                to_i = int(to_sock) if to_sock is not None else -1
            except (TypeError, ValueError):
                to_i = -1
            if to_i >= 0:
                to_cam = dai.CameraBoardSocket(to_i)
                parent_name = _camera_board_socket_name(to_cam)
                parent_frame = _frame_camera(tf_prefix, parent_name)
                try:
                    extr_mat = calib.getCameraExtrinsics(curr_cam, to_cam, False)
                    trans = calib.getCameraTranslationVector(curr_cam, to_cam, False)
                    em = np.asarray(extr_mat, dtype=np.float64)
                    if em.size >= 9:
                        R_lux = em.reshape(4, 4)[:3, :3] if em.size == 16 else em.reshape(3, 3)
                        R_ros = _lux_extrinsic_rotation_to_ros_camera_frame(R_lux)
                        tx, ty, tz = _translation_lux_optical_to_ros_rdf(trans)
                        out.append(_tf_rigid(parent_frame, child_frame, R_ros, (tx, ty, tz)))
                except Exception as ex:
                    logging.warning(
                        "[oak] TF: camera extrinsics %s -> %s unavailable (%s)",
                        curr_cam, to_cam, ex,
                    )
            else:
                out.append(_tf_rigid(tf_base_frame, child_frame, np.eye(3), (0.0, 0.0, 0.0)))
            add_optical_joint(sock_name)
            used_optical.add(sock_name)
    else:
        rgb_n = _camera_board_socket_name(dai.CameraBoardSocket.CAM_A)
        out.append(
            _tf_rigid(tf_base_frame, _frame_camera(tf_prefix, rgb_n), np.eye(3), (0.0, 0.0, 0.0))
        )
        add_optical_joint(rgb_n)
        used_optical.add(rgb_n)
        for curr, parent_sock, pname in (
            (dai.CameraBoardSocket.CAM_B, dai.CameraBoardSocket.CAM_A, rgb_n),
            (dai.CameraBoardSocket.CAM_C, dai.CameraBoardSocket.CAM_A, rgb_n),
        ):
            try:
                extr_mat = calib.getCameraExtrinsics(curr, parent_sock, False)
                trans = calib.getCameraTranslationVector(curr, parent_sock, False)
            except Exception:
                continue
            em = np.asarray(extr_mat, dtype=np.float64)
            if em.size < 9:
                continue
            R_lux = em.reshape(4, 4)[:3, :3] if em.size == 16 else em.reshape(3, 3)
            R_ros = _lux_extrinsic_rotation_to_ros_camera_frame(R_lux)
            tx, ty, tz = _translation_lux_optical_to_ros_rdf(trans)
            sn = _camera_board_socket_name(curr)
            if sn not in used_optical:
                out.append(_tf_rigid(_frame_camera(tf_prefix, pname), _frame_camera(tf_prefix, sn), R_ros, (tx, ty, tz)))
                add_optical_joint(sn)
                used_optical.add(sn)

    # IMU: depthai-ros uses getImuToCameraExtrinsics + fixed RDF quaternion.
    imu_frame = f"{tf_prefix}_imu_frame"
    imu_parent: str | None = None
    trans_imu: tuple[float, float, float] = (0.0, 0.0, 0.0)
    imu_extr = data.get("imuExtrinsics")
    if isinstance(imu_extr, dict):
        try:
            to_s = int(imu_extr.get("toCameraSocket", -1))
        except (TypeError, ValueError):
            to_s = -1
        if to_s >= 0:
            try:
                sock = dai.CameraBoardSocket(to_s)
                imu_parent = _frame_camera(tf_prefix, _camera_board_socket_name(sock))
                raw_imu = calib.getImuToCameraExtrinsics(sock, False)
                M = np.asarray(raw_imu, dtype=np.float64).reshape(4, 4)
                trans_imu = _translation_lux_optical_to_ros_rdf([M[0, 3], M[1, 3], M[2, 3]])
            except Exception:
                imu_parent = None

    if imu_parent is None:
        try:
            raw_imu = calib.getImuToCameraExtrinsics(dai.CameraBoardSocket.CAM_A, False)
            M = np.asarray(raw_imu, dtype=np.float64).reshape(4, 4)
            trans_imu = _translation_lux_optical_to_ros_rdf([M[0, 3], M[1, 3], M[2, 3]])
            imu_parent = _frame_camera(tf_prefix, _camera_board_socket_name(dai.CameraBoardSocket.CAM_A))
        except Exception:
            imu_parent = tf_base_frame
            logging.warning(
                "[oak] IMU extrinsics unavailable; publishing %s under %s with zero translation",
                imu_frame, imu_parent,
            )

    out.append(_tf_quat(imu_parent, imu_frame, Q_OPTICAL_FROM_CAMERA_FRAME, trans_imu))
    return out


def _log_tf_snapshots(
    tf_ch: FrameTransformsChannel | None,
    tf_static_ch: FrameTransformsChannel | None,
    snapshots: list[TfTransformSnapshot],
    ts: Timestamp | None,
    *,
    log_static: bool = True,
    log_tf: bool = True,
) -> None:
    if not snapshots or (tf_ch is None and tf_static_ch is None):
        return
    bundle = FrameTransforms(transforms=[s.to_msg(ts) for s in snapshots])
    if log_static and tf_static_ch is not None:
        tf_static_ch.log(bundle)
    if log_tf and tf_ch is not None:
        tf_ch.log(bundle)


def _dai_ts_to_foxglove(img: Any) -> Timestamp | None:
    try:
        ts = img.getTimestamp()
        total_ns = int(ts.total_seconds() * 1e9)
        return Timestamp(sec=total_ns // 1_000_000_000, nsec=total_ns % 1_000_000_000)
    except Exception:
        return None


def _imu_ts_to_foxglove(ts_any: Any) -> Timestamp:
    """Convert a DepthAI ``timedelta``-like timestamp to Foxglove ``Timestamp``."""
    try:
        total_ns = int(ts_any.total_seconds() * 1e9)
        return Timestamp(sec=total_ns // 1_000_000_000, nsec=total_ns % 1_000_000_000)
    except Exception:
        return Timestamp.now()


# JSON schema matching the IMU topic in `oak-luxonis-4d/main.py` (a flat
# subset of `sensor_msgs/Imu`). Foxglove auto-discovers the topic without
# needing a ROS bag or msgdef.
IMU_JSON_SCHEMA: dict[str, Any] = {
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
        "orientation": {
            "type": "object",
            "properties": {
                "x": {"type": "number"},
                "y": {"type": "number"},
                "z": {"type": "number"},
                "w": {"type": "number"},
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


# --------------------------------------------------------------------------- #
# Point cloud building                                                        #
# --------------------------------------------------------------------------- #

def _try_set_depth_units_meter(rgbd_node: Any) -> bool:
    """Try the various DepthAI APIs that request METER output."""
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


def _detect_meter_scale(z_values: np.ndarray) -> float:
    pos = np.abs(z_values[z_values > 0])
    if pos.size == 0:
        return 1.0
    median_z = float(np.median(pos))
    return 0.001 if median_z > MM_HEURISTIC_THRESHOLD_M else 1.0


def _build_pcl_message(
    pcl_data: Any,
    ts: Timestamp,
    frame_id: str,
    *,
    locked_scale_cell: list[Optional[float]],
) -> PointCloud | None:
    """Convert ``dai.PointCloudData`` to a Foxglove ``PointCloud`` (XYZ+RGBA).

    ``locked_scale_cell`` is a 1-element list used as a mutable closure for the
    mm/m scale autodetect: the first cloud picks the scale, subsequent clouds
    reuse it so we don't flap if Z briefly goes near-zero.
    """
    points: np.ndarray
    colors: np.ndarray | None = None
    try:
        result = pcl_data.getPointsRGB()
    except Exception:
        result = None
    if isinstance(result, tuple) and len(result) == 2:
        points, colors = result
    else:
        try:
            points = pcl_data.getPoints()
        except Exception:
            return None

    if not isinstance(points, np.ndarray) or points.size == 0:
        return None

    pts = points.reshape(-1, points.shape[-1])[:, :3].astype(np.float32, copy=False)
    if pts.size == 0:
        return None
    finite = np.isfinite(pts).all(axis=1)
    if not bool(finite.any()):
        return None
    pts = pts[finite]
    if isinstance(colors, np.ndarray) and colors.size:
        col = colors.reshape(-1, colors.shape[-1])
        if col.shape[0] >= pts.shape[0] + int((~finite).sum()):
            # If color array tracks the original (non-filtered) length we
            # apply the same mask; otherwise we just trim/drop colors below.
            col_full = col[: finite.size]
            colors = col_full[finite].astype(np.uint8, copy=False)
        else:
            colors = None
    else:
        colors = None

    if locked_scale_cell[0] is None:
        locked_scale_cell[0] = _detect_meter_scale(pts[:, 2])
    scale = locked_scale_cell[0] or 1.0
    if scale != 1.0:
        pts = pts * np.float32(scale)

    n = int(pts.shape[0])
    rec = np.empty(n, dtype=_POINT_RECORD_DTYPE)
    pts_c = np.ascontiguousarray(pts, dtype=np.float32)
    rec["x"] = pts_c[:, 0]
    rec["y"] = pts_c[:, 1]
    rec["z"] = pts_c[:, 2]
    if colors is not None and colors.shape[1] >= 3:
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


def _camera_build_with_fps(
    pipeline: Any, socket: Any, sensor_fps: float
) -> Any:
    """Create + build a ``Camera`` node with an explicit ``sensorFps``.

    Pinning ``sensorFps`` at build time matters when the same Camera node is
    later asked for more than one output (e.g. the RGBD-aligned color stream
    *and* a small NV12 stream for the H.264 video tap). Without a pinned
    sensor rate, a later ``requestOutput(... fps=...)`` at a lower rate can
    drag the entire sensor / ISP timeline down, which knocks the depth-color
    Sync node inside ``dai.node.RGBD`` out of alignment and prints
    ``"Sync node has been trying to sync for N messages"`` warnings.

    Falls back to a positional build if the bindings don't accept the kwarg.
    """
    node = pipeline.create(dai.node.Camera)
    fps_f = float(sensor_fps)
    if socket is None:
        try:
            return node.build(sensorFps=fps_f)
        except TypeError:
            return node.build()
    try:
        return node.build(socket, sensorFps=fps_f)
    except TypeError:
        try:
            return node.build(boardSocket=socket, sensorFps=fps_f)
        except TypeError:
            return node.build(socket)


def _build_rgbd_stereo(
    pipeline: Any, size: tuple[int, int], fps: int
) -> tuple[Any, Any]:
    """Stereo + color RGBD pipeline matching the Luxonis Rerun example.

    Returns ``(rgbd_node, color_camera_node)`` so callers can request additional
    outputs (e.g. a small NV12 stream for a host-side video encoder) from the
    same ``Camera`` node without re-acquiring CAM_A.

    Depth is aligned to color and the RGBD node consumes the aligned depth and
    the color stream; this is the configuration the example uses to set
    ``DepthUnit.METER``.

    All three cameras (color, left, right) are pinned to ``fps`` via
    ``sensorFps`` so a later video tap at a different ``requestOutput`` rate
    cannot slow the color sensor down and desync depth from color.
    """
    color = _camera_build_with_fps(pipeline, None, fps)
    left = _camera_build_with_fps(pipeline, dai.CameraBoardSocket.CAM_B, fps)
    right = _camera_build_with_fps(pipeline, dai.CameraBoardSocket.CAM_C, fps)
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
        out = color.requestOutput(size, dai.ImgFrame.Type.RGB888i, enableUndistortion=True)
        align = pipeline.create(dai.node.ImageAlign)
        stereo.depth.link(align.input)
        out.link(align.inputAlignTo)
        align.outputAligned.link(rgbd.inDepth)
    else:
        out = color.requestOutput(size, dai.ImgFrame.Type.RGB888i, dai.ImgResizeMode.CROP, fps, True)
        stereo.depth.link(rgbd.inDepth)
        out.link(stereo.inputAlignTo)
    out.link(rgbd.inColor)
    return rgbd, color


# --------------------------------------------------------------------------- #
# Public API                                                                  #
# --------------------------------------------------------------------------- #

@dataclass
class OakStreamerConfig:
    """Tunable parameters for ``OakStreamer``.

    Defaults are picked for the reBotArm demo: colored RGBD point cloud,
    small-resolution H.264 video, and 50 Hz IMU — all rooted at the URDF
    ``oak`` link with shared ``/tf``.
    """

    # TF naming (matches depthai-ros: {prefix}_{socket}_camera_optical_frame).
    tf_prefix: str = "oak"
    # Root frame that the camera tree attaches to. The reBotArm URDF defines an
    # `oak` link bolted on link5, so the default value here makes the device's
    # static TF tree slot in there naturally.
    tf_base_frame: str = "oak"

    # ---- Point cloud (RGBD) ------------------------------------------------
    pcl_topic: str = "/oak/depth/points"
    # If None, defaults to "{tf_prefix}_rgb_camera_optical_frame".
    pcl_frame_id: Optional[str] = None

    # Pipeline shape for the RGBD/depth path.
    rgbd_size: tuple[int, int] = DEFAULT_SIZE
    rgbd_fps: int = STEREO_DEFAULT_FPS
    ir_laser_intensity: float = 0.7  # 0..1; matches the Luxonis example default.

    # ---- Video (low-res H.264, hardware encoded on device) -----------------
    enable_video: bool = True
    video_topic: str = "/oak/rgb/video"
    # 1080p preview. The video tap is taken off the same color sensor as the
    # RGBD path, so the ISP just adds another scaler stage; bandwidth is fine
    # at H.264 and Foxglove decodes 1080p in software without trouble.
    video_size: tuple[int, int] = (1080, 720)
    # Lower FPS keeps bandwidth low and leaves the depth pipeline headroom.
    video_fps: int = 15
    # Default frame_id when None: "{tf_prefix}_rgb_camera_optical_frame".
    video_frame_id: Optional[str] = None

    # ---- IMU (accel + gyro at the same rate) -------------------------------
    enable_imu: bool = True
    imu_topic: str = "/oak/imu"
    imu_rate_hz: int = 50
    # Device-side batching reduces "host too slow" warnings under load.
    imu_batch_threshold: int = 5
    imu_max_batch_reports: int = 20
    # Per worker-loop drain cap so IMU bursts cannot starve vision streams.
    imu_max_packets_per_tick: int = 16
    # Default frame_id when None: "{tf_prefix}_imu_frame".
    imu_frame_id: Optional[str] = None

    # ---- TF -----------------------------------------------------------------
    # Pass the channels already created by the demo so OAK TF joins the URDF
    # TF on the same /tf topic. If left None, the streamer creates its own.
    tf_channel: Optional[FrameTransformsChannel] = None
    tf_static_channel: Optional[FrameTransformsChannel] = None
    publish_static_tf: bool = True       # publish once on connect
    publish_live_tf_each_cloud: bool = True  # re-stamp & re-publish with each cloud

    # ---- Worker behavior ----------------------------------------------------
    # Idle sleep when every queue is empty. 5 ms gives plenty of headroom for
    # 50 Hz IMU (every 20 ms) and 30 Hz clouds.
    poll_sleep_s: float = 0.005
    log_every_n_clouds: int = 60         # 0 disables periodic stats logging
    log_prefix: str = "[oak]"            # prepended to log lines

    # Optional pre-created channels. If None the streamer creates them itself.
    point_cloud_channel: Optional[PointCloudChannel] = None
    video_channel: Optional[CompressedVideoChannel] = None
    imu_channel: Optional[Channel] = None


@dataclass
class _OakStreamerState:
    """Mutable state owned by the worker thread; not user-facing."""

    thread: Optional[threading.Thread] = None
    stop_event: threading.Event = field(default_factory=threading.Event)
    started_event: threading.Event = field(default_factory=threading.Event)
    last_error: Optional[BaseException] = None
    clouds_published: int = 0
    video_frames_published: int = 0
    imu_samples_published: int = 0


class OakStreamer:
    """Background OAK-4 → Foxglove streamer.

    Usage
    -----
    >>> streamer = OakStreamer(OakStreamerConfig(tf_channel=tf_ch))
    >>> streamer.start()
    ...                         # main thread does its own thing
    >>> streamer.stop(timeout=5)

    The streamer opens *one* ``dai.Device`` and *one* ``dai.Pipeline`` in its
    worker thread, reads the device calibration to build a static TF tree at
    startup, and then publishes a colored point cloud on ``pcl_topic`` plus
    live-restamped TF on every received cloud. If DepthAI / the device is
    unavailable, ``start()`` succeeds, the worker logs a warning, and the
    thread exits cleanly — the rest of your app is unaffected.
    """

    def __init__(self, config: Optional[OakStreamerConfig] = None):
        import json as _json  # local import keeps top-level deps tight

        self.config = config or OakStreamerConfig()
        cfg = self.config

        # Point cloud channel
        self._pcl_channel: PointCloudChannel = (
            cfg.point_cloud_channel
            if cfg.point_cloud_channel is not None
            else PointCloudChannel(topic=cfg.pcl_topic)
        )

        # Video channel (only if enabled). Creating a channel reserves the
        # topic name on the WS server, so we skip it when disabled.
        self._video_channel: Optional[CompressedVideoChannel] = None
        if cfg.enable_video:
            self._video_channel = (
                cfg.video_channel
                if cfg.video_channel is not None
                else CompressedVideoChannel(topic=cfg.video_topic)
            )

        # IMU channel (custom JSON schema)
        self._imu_channel: Optional[Channel] = None
        if cfg.enable_imu:
            self._imu_channel = (
                cfg.imu_channel
                if cfg.imu_channel is not None
                else Channel(
                    topic=cfg.imu_topic,
                    message_encoding="json",
                    schema=Schema(
                        name="sensor_msgs.msg.ImuLike",
                        encoding="jsonschema",
                        data=_json.dumps(IMU_JSON_SCHEMA).encode("utf-8"),
                    ),
                )
            )

        # TF channels
        self._tf_channel: Optional[FrameTransformsChannel] = cfg.tf_channel
        self._tf_static_channel: Optional[FrameTransformsChannel] = cfg.tf_static_channel
        if self._tf_channel is None:
            self._tf_channel = FrameTransformsChannel(topic="/tf")
        if self._tf_static_channel is None:
            self._tf_static_channel = FrameTransformsChannel(topic="/tf_static")

        self._state = _OakStreamerState()
        self._log = logging.getLogger("oak_streamer")

    # ----- lifecycle ---------------------------------------------------------

    def start(self) -> None:
        """Spawn the worker thread. Returns immediately."""
        if self._state.thread is not None and self._state.thread.is_alive():
            return
        if not _DEPTHAI_AVAILABLE:
            self._log.warning(
                "%s depthai is not importable (%s); OakStreamer will not publish anything",
                self.config.log_prefix, _DEPTHAI_IMPORT_ERROR,
            )
            return
        self._state.stop_event.clear()
        self._state.started_event.clear()
        self._state.thread = threading.Thread(
            target=self._run, name="oak-streamer", daemon=True,
        )
        self._state.thread.start()

    def stop(self, timeout: float = 5.0) -> None:
        """Signal the worker to exit and join. Safe to call multiple times."""
        self._state.stop_event.set()
        t = self._state.thread
        if t is not None and t.is_alive():
            t.join(timeout=timeout)
            if t.is_alive():
                self._log.warning(
                    "%s worker did not exit within %.1fs", self.config.log_prefix, timeout,
                )
        self._state.thread = None

    @property
    def is_running(self) -> bool:
        t = self._state.thread
        return t is not None and t.is_alive()

    @property
    def clouds_published(self) -> int:
        return self._state.clouds_published

    @property
    def video_frames_published(self) -> int:
        return self._state.video_frames_published

    @property
    def imu_samples_published(self) -> int:
        return self._state.imu_samples_published

    @property
    def last_error(self) -> Optional[BaseException]:
        return self._state.last_error

    # ----- worker ------------------------------------------------------------

    def _resolved_pcl_frame_id(self) -> str:
        return (
            self.config.pcl_frame_id
            if self.config.pcl_frame_id
            else f"{self.config.tf_prefix}_rgb_camera_optical_frame"
        )

    def _resolved_video_frame_id(self) -> str:
        return (
            self.config.video_frame_id
            if self.config.video_frame_id
            else f"{self.config.tf_prefix}_rgb_camera_optical_frame"
        )

    def _resolved_imu_frame_id(self) -> str:
        return (
            self.config.imu_frame_id
            if self.config.imu_frame_id
            else f"{self.config.tf_prefix}_imu_frame"
        )

    def _attach_video_stream(self, pipeline: Any, color_cam: Any) -> Any:
        """Add a small-res H.264 video tap off ``color_cam``.

        Returns the output queue or ``None`` if the device / SDK refused.
        Failure here is non-fatal: the rest of the streamer keeps running.

        IMPORTANT: the video output is requested at ``rgbd_fps`` (the color
        sensor's pinned rate), not at ``video_fps``. Requesting a *different*
        rate from the same Camera node can drag the color sensor down — even
        with ``sensorFps`` pinned at build time — which desyncs the RGBD node
        and produces ``"Sync node has been trying to sync"`` warnings. The
        encoder's ``frameRate`` is set to the same value so the H.264 stream's
        labeled rate matches reality.
        """
        cfg = self.config
        if cfg.video_fps != cfg.rgbd_fps:
            self._log.warning(
                "%s video_fps=%d differs from rgbd_fps=%d; both color outputs "
                "share one sensor so the video tap will run at %d fps to keep "
                "the depth-color Sync node aligned. Lower rgbd_fps if you "
                "really need a slower stream.",
                cfg.log_prefix, cfg.video_fps, cfg.rgbd_fps, cfg.rgbd_fps,
            )
        effective_fps = cfg.rgbd_fps

        try:
            nv12_out = color_cam.requestOutput(
                cfg.video_size, dai.ImgFrame.Type.NV12, fps=effective_fps,
            )
        except Exception as ex:
            self._log.warning(
                "%s could not request NV12 video output @ %dx%d/%d fps (%s); video disabled",
                cfg.log_prefix, cfg.video_size[0], cfg.video_size[1], effective_fps, ex,
            )
            return None

        try:
            encoder = pipeline.create(dai.node.VideoEncoder).build(
                nv12_out,
                frameRate=effective_fps,
                profile=dai.VideoEncoderProperties.Profile.H264_MAIN,
            )
            q = encoder.out.createOutputQueue(maxSize=4, blocking=False)
        except Exception as ex:
            self._log.warning(
                "%s VideoEncoder setup failed (%s); video disabled", cfg.log_prefix, ex,
            )
            return None
        self._log.info(
            "%s video: %dx%d H.264 @ %d fps on %s",
            cfg.log_prefix, cfg.video_size[0], cfg.video_size[1], effective_fps, cfg.video_topic,
        )
        return q

    def _attach_imu_stream(self, pipeline: Any) -> Any:
        """Add an IMU node at ``cfg.imu_rate_hz``; return its output queue."""
        cfg = self.config
        try:
            imu = pipeline.create(dai.node.IMU)
            hz = max(1, min(int(cfg.imu_rate_hz), 500))
            imu.enableIMUSensor(dai.IMUSensor.ACCELEROMETER_UNCALIBRATED, hz)
            imu.enableIMUSensor(dai.IMUSensor.GYROSCOPE_UNCALIBRATED, hz)
            batch_thr = max(1, int(cfg.imu_batch_threshold))
            max_batch = max(batch_thr + 1, int(cfg.imu_max_batch_reports))
            imu.setBatchReportThreshold(batch_thr)
            imu.setMaxBatchReports(max_batch)
            q = imu.out.createOutputQueue(maxSize=50, blocking=False)
        except Exception as ex:
            self._log.warning(
                "%s IMU node setup failed (%s); IMU disabled", cfg.log_prefix, ex,
            )
            return None
        self._log.info(
            "%s IMU: accel/gyro @ %d Hz batchReportThreshold=%d maxBatchReports=%d on %s",
            cfg.log_prefix, hz, batch_thr, max_batch, cfg.imu_topic,
        )
        return q

    def _publish_video_frame(self, pkt: Any, frame_id: str) -> bool:
        """Pack an H.264 NAL packet into a Foxglove ``CompressedVideo``."""
        ts = _dai_ts_to_foxglove(pkt) or Timestamp.now()
        try:
            data = pkt.getData()
        except Exception:
            return False
        if data is None:
            return False
        blob = data.tobytes() if hasattr(data, "tobytes") else bytes(memoryview(data))
        if not blob:
            return False
        try:
            assert self._video_channel is not None
            self._video_channel.log(
                CompressedVideo(timestamp=ts, frame_id=frame_id, data=blob, format="h264")
            )
        except Exception as ex:
            self._log.warning("%s video publish failed: %s", self.config.log_prefix, ex)
            return False
        return True

    def _publish_imu_packets(self, ip: Any, frame_id: str) -> int:
        """Convert an ``IMUData`` batch into N JSON messages; returns N."""
        import json as _json

        published = 0
        max_n = max(1, int(self.config.imu_max_packets_per_tick))
        try:
            packets = ip.packets[:max_n]
        except Exception:
            return 0
        for imu_packet in packets:
            try:
                accel = imu_packet.acceleroMeter
                gyro = imu_packet.gyroscope
                ts = _imu_ts_to_foxglove(accel.getTimestamp())
                payload = {
                    "header": {
                        "stamp": {"sec": ts.sec, "nsec": ts.nsec},
                        "frame_id": frame_id,
                    },
                    "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0},
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
                assert self._imu_channel is not None
                self._imu_channel.log(_json.dumps(payload).encode("utf-8"))
                published += 1
            except Exception as ex:
                self._log.debug("%s IMU packet skipped: %s", self.config.log_prefix, ex)
                continue
        return published

    def _run(self) -> None:
        cfg = self.config
        pcl_frame_id = self._resolved_pcl_frame_id()
        video_frame_id = self._resolved_video_frame_id()
        imu_frame_id = self._resolved_imu_frame_id()
        log_prefix = cfg.log_prefix

        # 1) Open device, read calibration, build & publish static TF.
        tf_snaps: list[TfTransformSnapshot] = []
        try:
            with dai.Device() as device:  # type: ignore[union-attr]
                try:
                    calib = device.readCalibration2()
                    tf_snaps = build_tf_snapshots_from_calib(
                        calib, tf_prefix=cfg.tf_prefix, tf_base_frame=cfg.tf_base_frame,
                    )
                except Exception as ex:
                    self._log.warning("%s could not read device calibration: %s", log_prefix, ex)
        except Exception as ex:
            self._log.warning("%s could not open OAK device (%s); streamer will exit", log_prefix, ex)
            self._state.last_error = ex
            return

        if tf_snaps:
            self._log.info(
                "%s loaded %d static TF transforms from device calibration "
                "(rooted at %s, prefix %s)",
                log_prefix, len(tf_snaps), cfg.tf_base_frame, cfg.tf_prefix,
            )
            if cfg.publish_static_tf:
                _log_tf_snapshots(
                    self._tf_channel, self._tf_static_channel, tf_snaps,
                    Timestamp.now(), log_static=True, log_tf=True,
                )

        # 2) Build pipeline: RGBD + (optional) H.264 video tap + (optional) IMU.
        try:
            with dai.Pipeline() as pipeline:  # type: ignore[union-attr]
                rgbd, color_cam = _build_rgbd_stereo(pipeline, cfg.rgbd_size, cfg.rgbd_fps)
                if _try_set_depth_units_meter(rgbd):
                    self._log.info(
                        "%s RGBD: setDepthUnits(METER) succeeded (verified per-cloud)", log_prefix
                    )
                else:
                    self._log.warning(
                        "%s could not request METER units; relying on mm/m auto-detect", log_prefix
                    )

                pcl_queue = rgbd.pcl.createOutputQueue(maxSize=2, blocking=False)

                video_queue = None
                if cfg.enable_video and self._video_channel is not None:
                    video_queue = self._attach_video_stream(pipeline, color_cam)

                imu_queue = None
                if cfg.enable_imu and self._imu_channel is not None:
                    imu_queue = self._attach_imu_stream(pipeline)

                pipeline.start()

                # IR laser dot projector (matches the Luxonis Rerun example).
                try:
                    device = pipeline.getDefaultDevice()
                    if device is not None and hasattr(device, "setIrLaserDotProjectorIntensity"):
                        device.setIrLaserDotProjectorIntensity(float(cfg.ir_laser_intensity))
                except Exception as ex:
                    self._log.debug("%s setIrLaserDotProjectorIntensity skipped: %s", log_prefix, ex)

                self._log.info(
                    "%s pipeline running: RGBD %dx%d@%d on %s%s%s",
                    log_prefix,
                    cfg.rgbd_size[0], cfg.rgbd_size[1], cfg.rgbd_fps, cfg.pcl_topic,
                    f" + H.264 video on {cfg.video_topic}" if video_queue is not None else "",
                    f" + {cfg.imu_rate_hz} Hz IMU on {cfg.imu_topic}" if imu_queue is not None else "",
                )
                self._state.started_event.set()

                locked_scale_cell: list[Optional[float]] = [None]
                while not self._state.stop_event.is_set() and pipeline.isRunning():
                    progressed = False

                    # --- Point cloud ----------------------------------------
                    try:
                        pcl_data = pcl_queue.tryGet()
                    except Exception as ex:
                        self._log.warning("%s pcl queue error: %s", log_prefix, ex)
                        pcl_data = None
                    if pcl_data is not None:
                        progressed = True
                        ts = _dai_ts_to_foxglove(pcl_data) or Timestamp.now()
                        msg = _build_pcl_message(
                            pcl_data, ts, pcl_frame_id, locked_scale_cell=locked_scale_cell,
                        )
                        if msg is not None:
                            try:
                                self._pcl_channel.log(msg)
                                self._state.clouds_published += 1
                                if cfg.publish_live_tf_each_cloud and tf_snaps:
                                    _log_tf_snapshots(
                                        self._tf_channel, self._tf_static_channel,
                                        tf_snaps, ts, log_static=False, log_tf=True,
                                    )
                            except Exception as ex:
                                self._log.warning("%s PointCloud publish failed: %s", log_prefix, ex)

                    # --- Video (H.264) --------------------------------------
                    if video_queue is not None:
                        try:
                            vp = video_queue.tryGet()
                        except Exception as ex:
                            self._log.warning("%s video queue error: %s", log_prefix, ex)
                            vp = None
                        if vp is not None:
                            progressed = True
                            if self._publish_video_frame(vp, video_frame_id):
                                self._state.video_frames_published += 1

                    # --- IMU (drain up to imu_max_packets_per_tick) ---------
                    if imu_queue is not None:
                        try:
                            ip = imu_queue.tryGet()
                        except Exception as ex:
                            self._log.warning("%s imu queue error: %s", log_prefix, ex)
                            ip = None
                        if ip is not None and hasattr(ip, "packets"):
                            progressed = True
                            n = self._publish_imu_packets(ip, imu_frame_id)
                            self._state.imu_samples_published += n

                    if not progressed:
                        # Honor stop_event promptly while nothing is happening.
                        if self._state.stop_event.wait(cfg.poll_sleep_s):
                            break

                    n_every = cfg.log_every_n_clouds
                    if (
                        n_every > 0
                        and self._state.clouds_published > 0
                        and self._state.clouds_published % n_every == 0
                    ):
                        self._log.info(
                            "%s published: %d clouds, %d video frames, %d IMU samples "
                            "(scale=%.5g)",
                            log_prefix,
                            self._state.clouds_published,
                            self._state.video_frames_published,
                            self._state.imu_samples_published,
                            (locked_scale_cell[0] or 1.0),
                        )

                # Stop request: drain & stop the pipeline gracefully.
                try:
                    pipeline.stop()
                except Exception as ex:
                    self._log.debug("%s pipeline.stop raised: %s", log_prefix, ex)
        except Exception as ex:
            self._state.last_error = ex
            self._log.warning("%s pipeline error: %s", log_prefix, ex)
        finally:
            self._log.info(
                "%s worker exiting (published %d clouds, %d video frames, %d IMU samples)",
                log_prefix,
                self._state.clouds_published,
                self._state.video_frames_published,
                self._state.imu_samples_published,
            )
