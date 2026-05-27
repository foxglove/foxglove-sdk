#!/usr/bin/env python3
"""
Luxonis OAK-4 (DepthAI v3) → Foxglove WebSocket bridge.
"""

from __future__ import annotations

import argparse
import json
import logging
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import cv2
import depthai as dai
import foxglove
import numpy as np
from foxglove import Channel, Schema
from foxglove.channels import (
    CameraCalibrationChannel,
    CompressedVideoChannel,
    FrameTransformsChannel,
    PointCloudChannel,
    RawImageChannel,
)
from foxglove.messages import (
    CameraCalibration,
    CompressedVideo,
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

DEFAULT_CONFIG_PATH = Path(__file__).with_name("config.json")


@dataclass(frozen=True)
class CameraCalibSnapshot:
    """Holds calibration fields; Foxglove CameraCalibration has no Python property getters."""

    frame_id: str
    width: int
    height: int
    distortion_model: str
    D: tuple[float, ...]
    K: tuple[float, ...]
    R: tuple[float, ...]
    P: tuple[float, ...]

    def to_msg(self, ts: Timestamp | None = None) -> CameraCalibration:
        return CameraCalibration(
            timestamp=ts,
            frame_id=self.frame_id,
            width=self.width,
            height=self.height,
            distortion_model=self.distortion_model,
            D=list(self.D),
            K=list(self.K),
            R=list(self.R),
            P=list(self.P),
        )


@dataclass(frozen=True)
class TfTransformSnapshot:
    """Static TF from Luxonis getCameraExtrinsics / getCameraToImuExtrinsics."""

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


@dataclass
class FpsCounter:
    name: str
    window_s: float = 5.0
    _n: int = 0
    _t0: float = field(default_factory=time.monotonic)

    def tick(self) -> None:
        self._n += 1
        now = time.monotonic()
        if now - self._t0 >= self.window_s:
            hz = self._n / (now - self._t0)
            logging.info("%s ~%.1f Hz", self.name, hz)
            self._n = 0
            self._t0 = now


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


def _get_config_value(cfg: dict[str, Any], dotted_key: str, default: Any) -> Any:
    cur: Any = cfg
    for part in dotted_key.split("."):
        if not isinstance(cur, dict) or part not in cur:
            return default
        cur = cur[part]
    return cur


def _load_config(path: str) -> dict[str, Any]:
    config_path = Path(path).expanduser()
    if not config_path.is_absolute():
        config_path = Path.cwd() / config_path
    if not config_path.exists():
        return {}
    try:
        with config_path.open("r", encoding="utf-8") as f:
            loaded = json.load(f)
    except Exception as ex:
        raise SystemExit(f"Could not read config {config_path}: {ex}") from ex
    if not isinstance(loaded, dict):
        raise SystemExit(f"Config {config_path} must contain a JSON object")
    return loaded


def _add_toggle(
    parser: argparse.ArgumentParser,
    name: str,
    *,
    default_enabled: bool,
    help_enabled: str,
    help_disabled: str,
) -> None:
    dest = f"no_{name.replace('-', '_')}"
    parser.add_argument(
        f"--{name}",
        dest=dest,
        action="store_false",
        help=help_enabled,
    )
    parser.add_argument(
        f"--no-{name}",
        dest=dest,
        action="store_true",
        help=help_disabled,
    )
    parser.set_defaults(**{dest: not default_enabled})


def parse_args() -> argparse.Namespace:
    config_parser = argparse.ArgumentParser(add_help=False)
    config_parser.add_argument("--config", default=str(DEFAULT_CONFIG_PATH))
    config_args, _ = config_parser.parse_known_args()
    cfg = _load_config(config_args.config)

    p = argparse.ArgumentParser(description=__doc__, parents=[config_parser])
    p.add_argument("--rgb-width", type=int, default=_get_config_value(cfg, "camera.rgb_width", 1280))
    p.add_argument("--rgb-height", type=int, default=_get_config_value(cfg, "camera.rgb_height", 720))
    p.add_argument("--fps", type=int, default=_get_config_value(cfg, "camera.fps", 30))
    p.add_argument(
        "--stereo-width",
        type=int,
        default=_get_config_value(cfg, "stereo.width", 640),
    )
    p.add_argument(
        "--stereo-height",
        type=int,
        default=_get_config_value(cfg, "stereo.height", 400),
    )
    _add_toggle(
        p,
        "raw-rgb",
        default_enabled=bool(_get_config_value(cfg, "streams.raw_rgb", True)),
        help_enabled="Enable raw RGB preview (overrides config)",
        help_disabled="Disable raw RGB preview",
    )
    _add_toggle(
        p,
        "h264",
        default_enabled=bool(_get_config_value(cfg, "streams.h264", True)),
        help_enabled="Enable H.264 video (overrides config)",
        help_disabled="Disable H.264 video",
    )
    _add_toggle(
        p,
        "depth",
        default_enabled=bool(_get_config_value(cfg, "streams.depth", True)),
        help_enabled="Enable stereo depth (overrides config)",
        help_disabled="Disable stereo depth",
    )
    _add_toggle(
        p,
        "point-cloud",
        default_enabled=bool(_get_config_value(cfg, "streams.point_cloud", True)),
        help_enabled="Enable device-side point cloud (overrides config)",
        help_disabled="Disable device-side point cloud",
    )
    _add_toggle(
        p,
        "imu",
        default_enabled=bool(_get_config_value(cfg, "streams.imu", True)),
        help_enabled="Enable IMU publishing (overrides config)",
        help_disabled="Disable IMU publishing",
    )
    _add_toggle(
        p,
        "calibration",
        default_enabled=bool(_get_config_value(cfg, "streams.calibration", True)),
        help_enabled="Enable CameraCalibration topics (overrides config)",
        help_disabled="Disable all CameraCalibration topics",
    )
    p.add_argument(
        "--camera-info-timing",
        choices=("with_images", "once"),
        default=_get_config_value(cfg, "calibration.timing", "with_images"),
        help="Publish camera_info with each frame (synced timestamps) or once at startup",
    )
    _add_toggle(
        p,
        "tf",
        default_enabled=bool(_get_config_value(cfg, "streams.tf", True)),
        help_enabled="Enable FrameTransforms on /tf and /tf_static (overrides config)",
        help_disabled="Disable FrameTransforms on /tf and /tf_static",
    )
    p.add_argument(
        "--tf-prefix",
        default=_get_config_value(cfg, "tf.prefix", "oak"),
        help="TF frame prefix (matches depthai-ros: {prefix}_rgb_camera_optical_frame, etc.)",
    )
    p.add_argument(
        "--tf-base-frame",
        default=_get_config_value(cfg, "tf.base_frame", "oak"),
        help="Root frame attached to the camera rig (depthai-ros default base_frame)",
    )
    p.add_argument(
        "--tf-once",
        action="store_true",
        default=not bool(_get_config_value(cfg, "tf.continuous", True)),
        help="Publish /tf only once at startup (default: republish /tf on each RGB frame for live TF)",
    )
    p.add_argument(
        "--record",
        type=str,
        default=_get_config_value(cfg, "record", ""),
        help="Write MCAP to this path",
    )
    p.add_argument(
        "--imu-max-packets",
        type=int,
        default=_get_config_value(cfg, "imu.max_packets", 32),
        help="Max IMU samples to publish per IMU batch (prevents starving vision streams)",
    )
    p.add_argument(
        "--imu-accel-hz",
        type=int,
        default=_get_config_value(cfg, "imu.accel_hz", 100),
        metavar="HZ",
        help="Accelerometer report rate (Hz); lower = less USB load (e.g. 50, 100, 200)",
    )
    p.add_argument(
        "--imu-gyro-hz",
        type=int,
        default=_get_config_value(cfg, "imu.gyro_hz", 100),
        metavar="HZ",
        help="Gyroscope report rate (Hz); lower = less USB load (e.g. 50, 100, 200, 400)",
    )
    p.add_argument(
        "--imu-batch-threshold",
        type=int,
        default=_get_config_value(cfg, "imu.batch_threshold", 10),
        help="Device batches this many IMU samples before sending (reduces 'host too slow' warnings)",
    )
    p.add_argument(
        "--imu-max-batch-reports",
        type=int,
        default=_get_config_value(cfg, "imu.max_batch_reports", 40),
        help="Max IMU packets per device batch; must be > --imu-batch-threshold",
    )
    return p.parse_args()


def _intrinsics_matrix(
    calib: Any, socket: Any, width: int, height: int
) -> list[list[float]] | None:
    """Return 3x3 intrinsics as nested lists, or None."""
    M: Any = None
    try:
        try:
            M = calib.getCameraIntrinsics(socket, dai.Size2f(width, height))
        except Exception:
            try:
                M = calib.getCameraIntrinsics(socket, width, height)
            except Exception:
                M = calib.getCameraIntrinsics(socket, float(width), float(height))
    except Exception:
        return None
    if M is None or len(M) != 3:
        return None
    return [[float(M[r][c]) for c in range(3)] for r in range(3)]


def _distortion_list(calib: Any, socket: Any) -> list[float]:
    try:
        d = calib.getDistortionCoefficients(socket)
    except Exception:
        return []
    if d is None:
        return []
    return [float(x) for x in d]


def _distortion_model_for_d(D: list[float]) -> str:
    """Foxglove-supported models (see CameraCalibration.msg)."""
    n = len(D)
    if n <= 5:
        return "plumb_bob"
    if n <= 8:
        return "rational_polynomial"
    return "rational_polynomial"


def _k_flat(M: list[list[float]]) -> list[float]:
    return [float(M[r][c]) for r in range(3) for c in range(3)]


def _p_from_k_mono(K: list[float]) -> list[float]:
    fx, _, cx, _, fy, cy, _, _, _ = K
    return [fx, 0.0, cx, 0.0, 0.0, fy, cy, 0.0, 0.0, 0.0, 1.0, 0.0]


def build_rgb_camera_calibration(
    calib: Any, width: int, height: int, *, frame_id: str
) -> CameraCalibSnapshot | None:
    """Intrinsics for color (CAM_A) at output resolution — undistort / projection."""
    M = _intrinsics_matrix(calib, dai.CameraBoardSocket.CAM_A, width, height)
    if M is None:
        return None
    k = _k_flat(M)
    D = _distortion_list(calib, dai.CameraBoardSocket.CAM_A)
    dm = _distortion_model_for_d(D)
    r_ident = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    return CameraCalibSnapshot(
        frame_id=frame_id,
        width=width,
        height=height,
        distortion_model=dm,
        D=tuple(D),
        K=tuple(k),
        R=tuple(r_ident),
        P=tuple(_p_from_k_mono(k)),
    )


def build_depth_camera_calibration(
    calib: Any, width: int, height: int, *, frame_id: str
) -> CameraCalibSnapshot | None:
    """
    CameraCalibration for rectified stereo depth (same frame as /oak/depth/image).

    Uses OpenCV stereoRectify with Luxonis extrinsics so K/R/P match rectified depth.
    """
    Ml = _intrinsics_matrix(calib, dai.CameraBoardSocket.CAM_B, width, height)
    Mr = _intrinsics_matrix(calib, dai.CameraBoardSocket.CAM_C, width, height)
    if Ml is None or Mr is None:
        return None
    Dl = _distortion_list(calib, dai.CameraBoardSocket.CAM_B)
    Dr = _distortion_list(calib, dai.CameraBoardSocket.CAM_C)

    def _np_k(M: list[list[float]]) -> np.ndarray:
        return np.array(M, dtype=np.float64)

    def _np_d(D: list[float]) -> np.ndarray:
        return np.array(D, dtype=np.float64).reshape(-1, 1) if D else np.zeros((5, 1))

    # Extrinsics: rotation + translation from left (B) to right (C) camera coords.
    R_lr = np.eye(3, dtype=np.float64)
    T_lr = np.zeros((3, 1), dtype=np.float64)
    try:
        E = np.asarray(
            calib.getCameraExtrinsics(
                dai.CameraBoardSocket.CAM_B,
                dai.CameraBoardSocket.CAM_C,
            ),
            dtype=np.float64,
        )
        if E.size == 16:
            E = E.reshape(4, 4)
            R_lr = E[:3, :3]
            T_lr = E[:3, 3:4]
        elif E.size == 12:
            E = E.reshape(3, 4)
            R_lr = E[:, :3]
            T_lr = E[:, 3:4]
    except Exception as ex:
        logging.warning("Stereo extrinsics B→C unavailable (%s); using identity R, zero T", ex)

    image_size = (width, height)
    m1, d1, m2, d2 = _np_k(Ml), _np_d(Dl), _np_k(Mr), _np_d(Dr)

    def _rectify(R: np.ndarray, T: np.ndarray) -> tuple[Any, Any, Any, Any] | None:
        try:
            return cv2.stereoRectify(
                m1,
                d1,
                m2,
                d2,
                image_size,
                R,
                T,
                flags=cv2.CALIB_ZERO_DISPARITY,
                alpha=0.0,
            )
        except Exception:
            return None

    rect = _rectify(R_lr, T_lr)
    if rect is None:
        try:
            Ecb = np.asarray(
                calib.getCameraExtrinsics(
                    dai.CameraBoardSocket.CAM_C,
                    dai.CameraBoardSocket.CAM_B,
                ),
                dtype=np.float64,
            )
            if Ecb.size == 16:
                Ecb = Ecb.reshape(4, 4)
                R_cb = Ecb[:3, :3]
                T_cb = Ecb[:3, 3:4]
                R_sw = R_cb.T
                T_sw = -R_sw @ T_cb
                rect = _rectify(R_sw, T_sw)
        except Exception:
            rect = None

    if rect is None:
        logging.warning(
            "stereoRectify failed for both B→C and inverted C→B extrinsics; "
            "depth camera_info uses left intrinsics + device R_rect fallback",
        )
        k = _k_flat(Ml)
        try:
            r_rect = np.asarray(
                calib.getStereoLeftRectificationRotation(),
                dtype=np.float64,
            ).reshape(3, 3)
            R_flat = [float(r_rect[r, c]) for r in range(3) for c in range(3)]
        except Exception:
            R_flat = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        return CameraCalibSnapshot(
            frame_id=frame_id,
            width=width,
            height=height,
            distortion_model=_distortion_model_for_d(Dl),
            D=tuple(float(x) for x in Dl),
            K=tuple(k),
            R=tuple(R_flat),
            P=tuple(_p_from_k_mono(k)),
        )

    R1, R2, P1, P2, _Q, _roi1, _roi2 = rect

    # Rectified depth: zero distortion; K = left 3x3 of P1; R = identity.
    K_rect = P1[:, :3].astype(np.float64)
    k_rect = [float(K_rect[r, c]) for r in range(3) for c in range(3)]
    p_flat = [float(P1[r, c]) for r in range(3) for c in range(4)]
    r_ident = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    return CameraCalibSnapshot(
        frame_id=frame_id,
        width=width,
        height=height,
        distortion_model="plumb_bob",
        D=(0.0, 0.0, 0.0, 0.0, 0.0),
        K=tuple(k_rect),
        R=tuple(r_ident),
        P=tuple(p_flat),
    )


def rotation_matrix_to_quaternion(R: np.ndarray) -> tuple[float, float, float, float]:
    """3x3 rotation matrix -> (x, y, z, w) quaternion for Foxglove."""
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
    """Unit quaternion (x,y,z,w) -> 3x3 rotation matrix."""
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


def lux_extrinsic_rotation_to_ros_camera_frame(R_lux: np.ndarray) -> np.ndarray:
    """
    Same basis change as ``depthai_bridge::TFPublisher::quatFromRotM``:
    ``R_ros = R_spin @ R_lux @ R_spin^T`` with ``R_spin = R(q_rot2rdf)``.
    """
    q_spin = (-0.5, 0.5, -0.5, 0.5)
    R_spin = _quaternion_to_rotmat(q_spin[0], q_spin[1], q_spin[2], q_spin[3])
    R = np.asarray(R_lux, dtype=np.float64).reshape(3, 3)
    return R_spin @ R @ R_spin.T


def translation_lux_optical_to_ros_rdf(translation: Any) -> tuple[float, float, float]:
    """Match ``depthai_bridge::TFPublisher::transFromExtr`` (cm -> m, axis remap)."""
    t = np.asarray(translation, dtype=np.float64).reshape(-1)
    if t.size < 3:
        return 0.0, 0.0, 0.0
    x, y, z = float(t[0]), float(t[1]), float(t[2])
    return z / 100.0, x / -100.0, y / -100.0


def _camera_board_socket_name(sock: dai.CameraBoardSocket) -> str:
    """Names match ``depthai_bridge`` ``defaultSocketMap`` for typical OAK devices."""
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


# Fixed camera_frame -> camera_optical_frame rotation (RDF optical), from depthai-ros TFPublisher.
Q_OPTICAL_FROM_CAMERA_FRAME: tuple[float, float, float, float] = (-0.5, 0.5, -0.5, 0.5)


def _tf_snapshot_rigid(
    parent: str,
    child: str,
    R: np.ndarray,
    t_xyz: tuple[float, float, float],
) -> TfTransformSnapshot:
    qx, qy, qz, qw = rotation_matrix_to_quaternion(np.asarray(R, dtype=np.float64).reshape(3, 3))
    return TfTransformSnapshot(
        parent_frame_id=parent,
        child_frame_id=child,
        tx=t_xyz[0],
        ty=t_xyz[1],
        tz=t_xyz[2],
        qx=qx,
        qy=qy,
        qz=qz,
        qw=qw,
    )


def _tf_snapshot_quat_translation(
    parent: str,
    child: str,
    q: tuple[float, float, float, float],
    t_xyz: tuple[float, float, float],
) -> TfTransformSnapshot:
    qx, qy, qz, qw = q
    return TfTransformSnapshot(
        parent_frame_id=parent,
        child_frame_id=child,
        tx=t_xyz[0],
        ty=t_xyz[1],
        tz=t_xyz[2],
        qx=qx,
        qy=qy,
        qz=qz,
        qw=qw,
    )


def _eeprom_as_dict(calib: Any) -> dict[str, Any]:
    raw = calib.eepromToJson()
    if isinstance(raw, str):
        return json.loads(raw)
    if isinstance(raw, dict):
        return raw
    return dict(raw)


def build_tf_snapshots_from_calib(
    calib: Any, *, tf_prefix: str, tf_base_frame: str
) -> list[TfTransformSnapshot]:
    """
    TF tree aligned with ``depthai_bridge::TFPublisher`` (static URDF-style frames).

    Publishes ``{prefix}_{rgb|left|right}_camera_frame`` with mechanical extrinsics,
    each linked to ``{prefix}_*_camera_optical_frame`` via the standard optical rotation,
    plus ``{prefix}_imu_frame`` using ``getImuToCameraExtrinsics`` like the ROS driver.
    """
    out: list[TfTransformSnapshot] = []

    def add_optical_joint(socket_name: str) -> None:
        parent = _frame_camera(tf_prefix, socket_name)
        child = _frame_optical(tf_prefix, socket_name)
        q = Q_OPTICAL_FROM_CAMERA_FRAME
        out.append(
            _tf_snapshot_quat_translation(parent, child, q, (0.0, 0.0, 0.0))
        )

    data: dict[str, Any] = {}
    try:
        data = _eeprom_as_dict(calib)
        cam_data = data.get("cameraData")
    except Exception as ex:
        logging.warning("eepromToJson failed; using socket fallback for TF: %s", ex)
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
                        R_lux = (
                            em.reshape(4, 4)[:3, :3]
                            if em.size == 16
                            else em.reshape(3, 3)
                        )
                        R_ros = lux_extrinsic_rotation_to_ros_camera_frame(R_lux)
                        tx, ty, tz = translation_lux_optical_to_ros_rdf(trans)
                        out.append(
                            _tf_snapshot_rigid(
                                parent_frame, child_frame, R_ros, (tx, ty, tz)
                            )
                        )
                except Exception as ex:
                    logging.warning(
                        "TF: camera extrinsics %s → %s unavailable (%s)",
                        curr_cam,
                        to_cam,
                        ex,
                    )
            else:
                out.append(
                    _tf_snapshot_rigid(
                        tf_base_frame,
                        child_frame,
                        np.eye(3, dtype=np.float64),
                        (0.0, 0.0, 0.0),
                    )
                )
            add_optical_joint(sock_name)
            used_optical.add(sock_name)
    else:
        # Minimal stereo rig: rgb as root, left (and right) relative to rgb — same APIs as ROS.
        rgb_n = _camera_board_socket_name(dai.CameraBoardSocket.CAM_A)
        out.append(
            _tf_snapshot_rigid(
                tf_base_frame,
                _frame_camera(tf_prefix, rgb_n),
                np.eye(3, dtype=np.float64),
                (0.0, 0.0, 0.0),
            )
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
            R_ros = lux_extrinsic_rotation_to_ros_camera_frame(R_lux)
            tx, ty, tz = translation_lux_optical_to_ros_rdf(trans)
            sn = _camera_board_socket_name(curr)
            if sn not in used_optical:
                out.append(
                    _tf_snapshot_rigid(
                        _frame_camera(tf_prefix, pname),
                        _frame_camera(tf_prefix, sn),
                        R_ros,
                        (tx, ty, tz),
                    )
                )
                add_optical_joint(sn)
                used_optical.add(sn)

    # IMU: depthai-ros uses getImuToCameraExtrinsics + fixed RDF quaternion (not matrix R).
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
                trans_imu = translation_lux_optical_to_ros_rdf(
                    [M[0, 3], M[1, 3], M[2, 3]]
                )
            except Exception:
                imu_parent = None

    if imu_parent is None:
        try:
            raw_imu = calib.getImuToCameraExtrinsics(dai.CameraBoardSocket.CAM_A, False)
            M = np.asarray(raw_imu, dtype=np.float64).reshape(4, 4)
            trans_imu = translation_lux_optical_to_ros_rdf([M[0, 3], M[1, 3], M[2, 3]])
            imu_parent = _frame_camera(tf_prefix, _camera_board_socket_name(dai.CameraBoardSocket.CAM_A))
        except Exception:
            imu_parent = tf_base_frame
            logging.warning(
                "IMU extrinsics unavailable; publishing %s under %s with zero translation (ROS driver behavior).",
                imu_frame,
                imu_parent,
            )

    out.append(
        _tf_snapshot_quat_translation(
            imu_parent,
            imu_frame,
            Q_OPTICAL_FROM_CAMERA_FRAME,
            trans_imu,
        )
    )

    logging.info(
        "TF (depthai-ros style): %d transforms, prefix=%s base=%s",
        len(out),
        tf_prefix,
        tf_base_frame,
    )
    return out


def read_device_calibration_bundle(
    rgb_w: int,
    rgb_h: int,
    stereo_w: int,
    stereo_h: int,
    *,
    want_cal: bool,
    want_tf: bool,
    tf_prefix: str,
    tf_base_frame: str,
    rgb_optical_frame_id: str,
    depth_optical_frame_id: str,
) -> tuple[CameraCalibSnapshot | None, CameraCalibSnapshot | None, list[TfTransformSnapshot]]:
    """Load camera_info snapshots and/or TF from one ``dai.Device`` open."""
    rgb_cal: CameraCalibSnapshot | None = None
    depth_cal: CameraCalibSnapshot | None = None
    tf_snaps: list[TfTransformSnapshot] = []
    try:
        with dai.Device() as device:
            calib = device.readCalibration2()
            if want_cal:
                rgb_cal = build_rgb_camera_calibration(
                    calib, rgb_w, rgb_h, frame_id=rgb_optical_frame_id
                )
                depth_cal = build_depth_camera_calibration(
                    calib, stereo_w, stereo_h, frame_id=depth_optical_frame_id
                )
            if want_tf:
                tf_snaps = build_tf_snapshots_from_calib(
                    calib, tf_prefix=tf_prefix, tf_base_frame=tf_base_frame
                )
    except Exception as ex:
        logging.warning("Could not read device calibration / TF: %s", ex)
        return None, None, []
    return rgb_cal, depth_cal, tf_snaps


def log_tf_snapshots(
    tf_ch: FrameTransformsChannel | None,
    tf_static_ch: FrameTransformsChannel | None,
    snapshots: list[TfTransformSnapshot],
    ts: Timestamp | None,
    *,
    log_static: bool = True,
    log_tf: bool = True,
) -> None:
    """Publish device extrinsics as ``FrameTransforms`` (ROS-style parent/child)."""
    if not snapshots or (tf_ch is None and tf_static_ch is None):
        return
    transforms = [s.to_msg(ts) for s in snapshots]
    bundle = FrameTransforms(transforms=transforms)
    if log_static and tf_static_ch is not None:
        tf_static_ch.log(bundle)
    if log_tf and tf_ch is not None:
        tf_ch.log(bundle)


def dai_ts_to_foxglove(img: dai.ImgFrame) -> Timestamp | None:
    try:
        ts = img.getTimestamp()
        total_ns = int(ts.total_seconds() * 1e9)
        return Timestamp(sec=total_ns // 1_000_000_000, nsec=total_ns % 1_000_000_000)
    except Exception:
        return None


def imu_ts_to_foxglove(ts_any: Any) -> Timestamp:
    try:
        total_ns = int(ts_any.total_seconds() * 1e9)
        return Timestamp(sec=total_ns // 1_000_000_000, nsec=total_ns % 1_000_000_000)
    except Exception:
        return Timestamp.now()


def imgframe_to_bgr8(img: dai.ImgFrame) -> tuple[np.ndarray, int, int] | None:
    """Return (bgr_hwc, width, height) for Foxglove bgr8, or None if conversion fails."""
    try:
        frame = img.getCvFrame()
    except Exception as ex:
        logging.warning("getCvFrame failed: %s", ex)
        return None
    if frame is None or not isinstance(frame, np.ndarray) or frame.size == 0:
        return None
    h, w = frame.shape[:2]
    if frame.ndim == 2:
        # Mono / unexpected — expand to BGR for Image panel
        frame = cv2.cvtColor(frame, cv2.COLOR_GRAY2BGR)
        h, w = frame.shape[:2]
    elif frame.shape[2] == 4:
        frame = cv2.cvtColor(frame, cv2.COLOR_BGRA2BGR)
        h, w = frame.shape[:2]
    elif frame.shape[2] == 3:
        pass  # assume BGR from DepthAI
    else:
        return None
    return frame, w, h


def depth_imgframe_to_raw16_packed(dp: dai.ImgFrame) -> tuple[bytes, int, int, int] | None:
    """
    Build a tightly packed uint16 depth buffer for ``foxglove.RawImage`` (``16UC1``).

    Uses ``getWidth`` / ``getHeight`` / ``getStride`` so row padding from the device is
    not misinterpreted as pixel data (a common cause of diagonal shear / striping when
    ``getFrame().tobytes()`` is used blindly on strided buffers).
    """
    try:
        w = int(dp.getWidth())
        h = int(dp.getHeight())
    except Exception:
        return None
    if w <= 0 or h <= 0:
        return None

    try:
        stride = int(dp.getStride())
    except Exception:
        stride = 0
    row_bytes = w * 2
    if stride < row_bytes:
        stride = row_bytes

    # Prefer raw host buffer + stride (matches Luxonis ImgFrame layout).
    try:
        raw = dp.getData()
        if raw is not None:
            buf = (
                raw.tobytes()
                if hasattr(raw, "tobytes")
                else bytes(memoryview(raw))
            )
            min_len = stride * h
            if len(buf) >= min_len:
                if stride == row_bytes:
                    return buf[: min_len], w, h, row_bytes
                out = bytearray(h * row_bytes)
                mv = memoryview(buf)
                for r in range(h):
                    row = mv[r * stride : r * stride + row_bytes]
                    out[r * row_bytes : (r + 1) * row_bytes] = row
                return bytes(out), w, h, row_bytes
            if len(buf) >= h * row_bytes:
                # Tightly packed buffer without per-row padding
                return buf[: h * row_bytes], w, h, row_bytes
    except Exception:
        pass

    try:
        fr = dp.getFrame()
    except Exception:
        return None
    if not isinstance(fr, np.ndarray) or fr.size == 0:
        return None
    if fr.ndim == 3 and fr.shape[2] == 1:
        fr = fr[:, :, 0]
    if fr.ndim != 2:
        return None

    if fr.dtype == np.float32 or fr.dtype == np.float64:
        fr = np.nan_to_num(fr, nan=0.0, posinf=0.0, neginf=0.0)
        fr = np.clip(np.rint(fr), 0, 65535).astype(np.uint16)
    elif fr.dtype != np.uint16:
        fr = fr.astype(np.uint16, copy=False)

    rh, rw = int(fr.shape[0]), int(fr.shape[1])
    if rh != h or rw != w:
        if fr.size == w * h:
            fr = fr.reshape((h, w))
        else:
            return None

    fr = np.ascontiguousarray(fr)
    return fr.tobytes(), w, h, row_bytes


POINT_CLOUD_FIELDS = [
    PackedElementField(name="x", offset=0, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="y", offset=4, type=PackedElementFieldNumericType.Float32),
    PackedElementField(name="z", offset=8, type=PackedElementFieldNumericType.Float32),
]
POINT_CLOUD_POSE = Pose(
    position=Vector3(x=0.0, y=0.0, z=0.0),
    orientation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
)
POINT_CLOUD_MM_HEURISTIC_THRESHOLD_M = 50.0
_point_cloud_auto_scale_warned = False


def depthai_length_unit_to_meters(unit: Any) -> float:
    """Scale DepthAI PointCloud coordinates into Foxglove's meter-based scene."""
    unit_scales = {
        "METER": 1.0,
        "CENTIMETER": 0.01,
        "MILLIMETER": 0.001,
        "INCH": 0.0254,
        "FOOT": 0.3048,
    }
    for name, scale in unit_scales.items():
        if unit == getattr(dai.LengthUnit, name, None):
            return scale
    name = getattr(unit, "name", str(unit)).rsplit(".", maxsplit=1)[-1].upper()
    return unit_scales.get(name, 1.0)


def pointcloud_scale_to_meters(xyz: np.ndarray, configured_scale: float) -> float:
    """
    Return the scale to publish DepthAI XYZ in meters.

    Some device-side PointCloud paths still emit millimeter-scale coordinates even
    after accepting METER configuration, so verify the actual depth magnitude.
    """
    if configured_scale != 1.0:
        return configured_scale
    positive_z = np.abs(xyz[:, 2][xyz[:, 2] > 0])
    if positive_z.size == 0:
        return configured_scale
    median_z = float(np.median(positive_z))
    if median_z > POINT_CLOUD_MM_HEURISTIC_THRESHOLD_M:
        global _point_cloud_auto_scale_warned
        if not _point_cloud_auto_scale_warned:
            logging.warning(
                "PointCloud: DepthAI reported meter output, but median Z is %.3f; "
                "treating coordinates as millimeters and scaling to meters for Foxglove",
                median_z,
            )
            _point_cloud_auto_scale_warned = True
        return 0.001
    return configured_scale


def pointcloud_data_to_msg(
    pcl_data: Any,
    ts: Timestamp,
    frame_id: str,
    unit_scale_to_meters: float,
) -> PointCloud | None:
    """
    Convert device-generated ``dai.PointCloudData`` to Foxglove ``PointCloud``.

    DepthAI's configured output unit is converted to meters for Foxglove.
    """
    try:
        points = pcl_data.getPoints()
    except Exception:
        return None
    if not isinstance(points, np.ndarray) or points.size == 0:
        return None
    if points.ndim == 3 and points.shape[2] >= 3:
        points = points.reshape((-1, points.shape[2]))
    if points.ndim != 2 or points.shape[1] < 3:
        return None
    xyz = points[:, :3]
    finite = np.isfinite(xyz).all(axis=1)
    if not bool(finite.any()):
        return None
    xyz = xyz[finite]
    if xyz.dtype != np.float32:
        xyz = xyz.astype(np.float32, copy=False)
    scale_to_meters = pointcloud_scale_to_meters(xyz, unit_scale_to_meters)
    if scale_to_meters != 1.0:
        xyz = xyz * np.float32(scale_to_meters)
    xyz = np.ascontiguousarray(xyz)
    return PointCloud(
        timestamp=ts,
        frame_id=frame_id,
        pose=POINT_CLOUD_POSE,
        point_stride=12,
        fields=POINT_CLOUD_FIELDS,
        data=xyz.tobytes(),
    )


def main() -> None:
    args = parse_args()
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    foxglove.set_log_level(logging.INFO)

    want_cal = not args.no_calibration
    want_tf = not args.no_tf
    want_point_cloud = not args.no_point_cloud

    rgb_optical = f"{args.tf_prefix}_rgb_camera_optical_frame"
    depth_optical = f"{args.tf_prefix}_left_camera_optical_frame"
    imu_frame_id = f"{args.tf_prefix}_imu_frame"

    rgb_ch = RawImageChannel(topic="/oak/rgb/image_raw")
    vid_ch = CompressedVideoChannel(topic="/oak/rgb/video")
    depth_ch = RawImageChannel(topic="/oak/depth/image")
    point_cloud_ch = PointCloudChannel(topic="/oak/depth/points")
    rgb_cal_ch = CameraCalibrationChannel(topic="/oak/rgb/camera_info")
    depth_cal_ch = CameraCalibrationChannel(topic="/oak/depth/camera_info")
    tf_ch: FrameTransformsChannel | None = None
    tf_static_ch: FrameTransformsChannel | None = None
    if want_tf:
        tf_ch = FrameTransformsChannel(topic="/tf")
        tf_static_ch = FrameTransformsChannel(topic="/tf_static")

    imu_chan = Channel(
        topic="/oak/imu",
        message_encoding="json",
        schema=Schema(
            name="sensor_msgs.msg.ImuLike",
            encoding="jsonschema",
            data=json.dumps(IMU_JSON_SCHEMA).encode("utf-8"),
        ),
    )

    writer = foxglove.open_mcap(args.record) if args.record else None

    rgb_cal_template: CameraCalibSnapshot | None = None
    depth_cal_template: CameraCalibSnapshot | None = None
    tf_snapshots: list[TfTransformSnapshot] = []
    if want_cal or want_tf:
        rgb_cal_template, depth_cal_template, tf_snapshots = read_device_calibration_bundle(
            args.rgb_width,
            args.rgb_height,
            args.stereo_width,
            args.stereo_height,
            want_cal=want_cal,
            want_tf=want_tf,
            tf_prefix=args.tf_prefix,
            tf_base_frame=args.tf_base_frame,
            rgb_optical_frame_id=rgb_optical,
            depth_optical_frame_id=depth_optical,
        )
        if want_cal and rgb_cal_template is not None:
            logging.info(
                "RGB camera_info: %s, %dx%d, D len=%d",
                rgb_cal_template.distortion_model,
                rgb_cal_template.width,
                rgb_cal_template.height,
                len(rgb_cal_template.D),
            )
        if want_cal and depth_cal_template is not None and not args.no_depth:
            logging.info(
                "Depth camera_info: %s, %dx%d",
                depth_cal_template.distortion_model,
                depth_cal_template.width,
                depth_cal_template.height,
            )
        elif want_cal and not args.no_depth and depth_cal_template is None:
            logging.warning("Depth camera_info unavailable (check stereo calibration)")
        if want_tf and not tf_snapshots:
            logging.warning("No TF snapshots from device (depth<->RGB / IMU extrinsics missing)")

        if want_cal and args.camera_info_timing == "once":
            if rgb_cal_template is not None:
                rgb_cal_ch.log(rgb_cal_template.to_msg(None))
                logging.info("Published /oak/rgb/camera_info (once)")
            if depth_cal_template is not None and not args.no_depth:
                depth_cal_ch.log(depth_cal_template.to_msg(None))
                logging.info("Published /oak/depth/camera_info (once)")

    server = foxglove.start_server()
    logging.info("Foxglove: %s", server.app_url())

    if want_tf and tf_snapshots:
        log_tf_snapshots(
            tf_ch,
            tf_static_ch,
            tf_snapshots,
            Timestamp.now(),
            log_static=True,
            log_tf=True,
        )
        logging.info(
            "Published device TF (%d transforms) on /tf and /tf_static",
            len(tf_snapshots),
        )

    fps_rgb = FpsCounter("rgb")
    fps_enc = FpsCounter("h264")
    fps_depth = FpsCounter("depth")
    fps_pcl = FpsCounter("point_cloud")
    fps_imu = FpsCounter("imu")

    publish_cal_with_images = (
        not args.no_calibration
        and args.camera_info_timing == "with_images"
    )
    publish_tf_live = want_tf and bool(tf_snapshots) and not args.tf_once

    with dai.Pipeline() as pipeline:
        rgb_q: Any = None
        enc_q: Any = None
        depth_q: Any = None
        point_cloud_q: Any = None
        point_cloud_unit_scale_to_meters = 0.001
        imu_q: Any = None

        color = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_A)

        # One NV12 ISP output fan-outs to VideoEncoder + host (see depthai video_encode example).
        # Separate BGR888p + NV12 requests can fail or starve on some OAK-4 / RVC4 setups.
        color_nv12 = None
        if not args.no_raw_rgb or not args.no_h264:
            resize_mode = getattr(dai, "ImgResizeMode", None)
            letterbox = (
                getattr(resize_mode, "LETTERBOX", None)
                if resize_mode is not None
                else None
            )
            ro_kwargs: dict[str, Any] = {
                "size": (args.rgb_width, args.rgb_height),
                "type": dai.ImgFrame.Type.NV12,
                "fps": args.fps,
            }
            if letterbox is not None:
                ro_kwargs["resize_mode"] = letterbox
            try:
                color_nv12 = color.requestOutput(**ro_kwargs)
            except TypeError:
                del ro_kwargs["resize_mode"]
                color_nv12 = color.requestOutput(**ro_kwargs)

        if not args.no_raw_rgb and color_nv12 is not None:
            rgb_q = color_nv12.createOutputQueue(maxSize=2, blocking=False)

        if not args.no_h264 and color_nv12 is not None:
            encoder = pipeline.create(dai.node.VideoEncoder).build(
                color_nv12,
                frameRate=args.fps,
                profile=dai.VideoEncoderProperties.Profile.H264_MAIN,
            )
            enc_q = encoder.out.createOutputQueue(maxSize=4, blocking=False)

        depth_pipeline_enabled = not args.no_depth or want_point_cloud
        point_cloud_available = want_point_cloud
        if want_point_cloud and not hasattr(dai.node, "PointCloud"):
            logging.warning(
                "Point cloud enabled, but this DepthAI build has no dai.node.PointCloud; "
                "not computing point cloud on host."
            )
            point_cloud_available = False

        if depth_pipeline_enabled:
            left = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_B)
            right = pipeline.create(dai.node.Camera).build(dai.CameraBoardSocket.CAM_C)
            stereo = pipeline.create(dai.node.StereoDepth)
            l_out = left.requestOutput(
                (args.stereo_width, args.stereo_height),
                type=dai.ImgFrame.Type.GRAY8,
                fps=args.fps,
            )
            r_out = right.requestOutput(
                (args.stereo_width, args.stereo_height),
                type=dai.ImgFrame.Type.GRAY8,
                fps=args.fps,
            )
            l_out.link(stereo.left)
            r_out.link(stereo.right)
            stereo.setRectification(True)
            stereo.setExtendedDisparity(True)
            stereo.setLeftRightCheck(True)
            if hasattr(stereo, "setEnableFrameSync"):
                try:
                    stereo.setEnableFrameSync(True)
                except Exception:
                    pass
            if not args.no_depth:
                depth_q = stereo.depth.createOutputQueue(maxSize=4, blocking=False)

            if point_cloud_available:
                point_cloud = pipeline.create(dai.node.PointCloud)
                if hasattr(point_cloud, "setRunOnHost"):
                    try:
                        point_cloud.setRunOnHost(False)
                    except Exception as ex:
                        logging.warning(
                            "Point cloud enabled, but device-side PointCloud failed (%s); "
                            "not computing point cloud on host.",
                            ex,
                        )
                        point_cloud_available = False
                else:
                    logging.warning(
                        "Point cloud enabled, but DepthAI PointCloud cannot be forced to run on device; "
                        "not computing point cloud on host."
                    )
                    point_cloud_available = False

                if point_cloud_available:
                    if hasattr(point_cloud, "initialConfig"):
                        try:
                            point_cloud.initialConfig.setLengthUnit(dai.LengthUnit.METER)
                            unit = (
                                point_cloud.initialConfig.getLengthUnit()
                                if hasattr(point_cloud.initialConfig, "getLengthUnit")
                                else dai.LengthUnit.METER
                            )
                            point_cloud_unit_scale_to_meters = depthai_length_unit_to_meters(unit)
                        except Exception as ex:
                            logging.warning(
                                "PointCloud: could not set DepthAI output unit to meters (%s); "
                                "assuming default millimeters and converting for Foxglove",
                                ex,
                            )
                    stereo.depth.link(point_cloud.inputDepth)
                    point_cloud_q = point_cloud.outputPointCloud.createOutputQueue(
                        maxSize=2,
                        blocking=False,
                    )
                    logging.info(
                        "PointCloud: device-side DepthAI node enabled on /oak/depth/points "
                        "(scale to meters: %.6g)",
                        point_cloud_unit_scale_to_meters,
                    )

        if not args.no_imu:
            imu = pipeline.create(dai.node.IMU)
            accel_hz = max(1, min(args.imu_accel_hz, 500))
            gyro_hz = max(1, min(args.imu_gyro_hz, 500))
            imu.enableIMUSensor(
                dai.IMUSensor.ACCELEROMETER_UNCALIBRATED,
                accel_hz,
            )
            imu.enableIMUSensor(
                dai.IMUSensor.GYROSCOPE_UNCALIBRATED,
                gyro_hz,
            )
            batch_thr = max(1, args.imu_batch_threshold)
            max_batch = max(batch_thr + 1, args.imu_max_batch_reports)
            imu.setBatchReportThreshold(batch_thr)
            imu.setMaxBatchReports(max_batch)
            imu_q = imu.out.createOutputQueue(maxSize=50, blocking=False)
            logging.info(
                "IMU: accel=%d Hz gyro=%d Hz batchReportThreshold=%d maxBatchReports=%d",
                accel_hz,
                gyro_hz,
                batch_thr,
                max_batch,
            )

        pipeline.start()
        logging.info("DepthAI pipeline running — Ctrl+C to stop")

        rgb_warned = False
        depth_warned_payload = False
        depth_warned_type = False
        point_cloud_warned = False

        try:
            while pipeline.isRunning():
                # Drain vision first — IMU can produce hundreds of samples per batch.
                for _ in range(32):
                    progressed = False
                    republish_tf_ts: Timestamp | None = None
                    if rgb_q is not None:
                        pkt = rgb_q.tryGet()
                        if pkt is not None and isinstance(pkt, dai.ImgFrame):
                            progressed = True
                            ts = dai_ts_to_foxglove(pkt) or Timestamp.now()
                            converted = imgframe_to_bgr8(pkt)
                            if converted is None:
                                if not rgb_warned:
                                    logging.warning(
                                        "RGB: could not convert frame (type=%s); check Camera NV12 / OpenCV",
                                        pkt.getType(),
                                    )
                                    rgb_warned = True
                            else:
                                frame, w, h = converted
                                rgb_ch.log(
                                    RawImage(
                                        timestamp=ts,
                                        frame_id=rgb_optical,
                                        width=w,
                                        height=h,
                                        encoding="bgr8",
                                        step=w * 3,
                                        data=frame.tobytes(),
                                    )
                                )
                                if publish_cal_with_images and rgb_cal_template is not None:
                                    rgb_cal_ch.log(rgb_cal_template.to_msg(ts))
                                republish_tf_ts = ts
                                fps_rgb.tick()

                    if enc_q is not None:
                        ep = enc_q.tryGet()
                        if ep is not None:
                            progressed = True
                            ts = Timestamp.now()
                            blob = b""
                            if isinstance(ep, dai.ImgFrame):
                                ts = dai_ts_to_foxglove(ep) or ts
                                data = ep.getData()
                                blob = (
                                    data.tobytes()
                                    if hasattr(data, "tobytes")
                                    else bytes(memoryview(data))
                                )
                            elif hasattr(ep, "getData"):
                                data = ep.getData()
                                blob = (
                                    data.tobytes()
                                    if hasattr(data, "tobytes")
                                    else bytes(memoryview(data))
                                )
                            if blob:
                                vid_ch.log(
                                    CompressedVideo(
                                        timestamp=ts,
                                        frame_id=rgb_optical,
                                        data=blob,
                                        format="h264",
                                    )
                                )
                                if republish_tf_ts is None:
                                    republish_tf_ts = ts
                                fps_enc.tick()

                    if depth_q is not None:
                        dp = depth_q.tryGet()
                        if dp is not None and isinstance(dp, dai.ImgFrame):
                            progressed = True
                            ts = dai_ts_to_foxglove(dp) or Timestamp.now()
                            raw16 = depth_imgframe_to_raw16_packed(dp)
                            if raw16 is None:
                                if not depth_warned_payload:
                                    try:
                                        meta = (
                                            f"type={dp.getType()} "
                                            f"{dp.getWidth()}x{dp.getHeight()} "
                                            f"stride={dp.getStride()}"
                                        )
                                    except Exception:
                                        meta = "(metadata unavailable)"
                                    logging.warning(
                                        "Depth: could not pack RawImage (%s); "
                                        "check StereoDepth depth output / dtype",
                                        meta,
                                    )
                                    depth_warned_payload = True
                            else:
                                blob, dw, dh, step_b = raw16
                                raw_t = getattr(dai.ImgFrame.Type, "RAW16", None)
                                if (
                                    raw_t is not None
                                    and hasattr(dp, "getType")
                                    and dp.getType() != raw_t
                                    and not depth_warned_type
                                ):
                                    logging.info(
                                        "Depth ImgFrame type=%s (device often uses RAW16 mm); "
                                        "pixels were cast/normalized to uint16 for Foxglove",
                                        dp.getType(),
                                    )
                                    depth_warned_type = True
                                depth_ch.log(
                                    RawImage(
                                        timestamp=ts,
                                        frame_id=depth_optical,
                                        width=dw,
                                        height=dh,
                                        encoding="16UC1",
                                        step=step_b,
                                        data=blob,
                                    )
                                )
                                if republish_tf_ts is None:
                                    republish_tf_ts = ts
                                if publish_cal_with_images and depth_cal_template is not None:
                                    depth_cal_ch.log(depth_cal_template.to_msg(ts))
                                fps_depth.tick()

                    if point_cloud_q is not None:
                        pcl_data = point_cloud_q.tryGet()
                        if pcl_data is not None:
                            progressed = True
                            try:
                                ts = imu_ts_to_foxglove(pcl_data.getTimestamp())
                            except Exception:
                                ts = republish_tf_ts or Timestamp.now()
                            pcl_msg = pointcloud_data_to_msg(
                                pcl_data,
                                ts,
                                depth_optical,
                                point_cloud_unit_scale_to_meters,
                            )
                            if pcl_msg is None:
                                if not point_cloud_warned:
                                    logging.warning(
                                        "PointCloud: device output could not be serialized to Foxglove PointCloud"
                                    )
                                    point_cloud_warned = True
                            else:
                                point_cloud_ch.log(pcl_msg)
                                if republish_tf_ts is None:
                                    republish_tf_ts = ts
                                fps_pcl.tick()

                    if (
                        publish_tf_live
                        and republish_tf_ts is not None
                        and tf_ch is not None
                    ):
                        log_tf_snapshots(
                            tf_ch,
                            tf_static_ch,
                            tf_snapshots,
                            republish_tf_ts,
                            log_static=False,
                            log_tf=True,
                        )

                    if not progressed:
                        break

                if imu_q is not None:
                    ip = imu_q.tryGet()
                    if ip is not None and isinstance(ip, dai.IMUData):
                        max_n = max(1, args.imu_max_packets)
                        for imu_packet in ip.packets[:max_n]:
                            accel = imu_packet.acceleroMeter
                            gyro = imu_packet.gyroscope
                            ts = imu_ts_to_foxglove(accel.getTimestamp())
                            payload = {
                                "header": {
                                    "stamp": {"sec": ts.sec, "nsec": ts.nsec},
                                    "frame_id": imu_frame_id,
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
                            imu_chan.log(json.dumps(payload).encode("utf-8"))
                            fps_imu.tick()
        except KeyboardInterrupt:
            logging.info("Stopping…")
        finally:
            pipeline.stop()

    if writer is not None:
        writer.close()
        logging.info("MCAP closed: %s", args.record)


if __name__ == "__main__":
    main()
