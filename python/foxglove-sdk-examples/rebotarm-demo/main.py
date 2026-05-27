#!/usr/bin/env python3
"""reBotArm demo mode (slow sinusoidal sway around a home pose).

Uses POS_VEL mode in two phases:

  1. Homing: slowly drive every joint from its starting pose to a fixed
     HOME_POSE_DEG. The operator must press Enter to confirm that this
     straight joint-space move is safe (no self-collision, no obstacles).
  2. Oscillation: drive the 6 joints in a low-amplitude, low-frequency,
     phase-shifted sinusoidal motion centered on HOME_POSE_DEG.

Control law during oscillation:
    q_target[i] = home[i] + ramp(t) * amplitude[i] * sin(2*pi*f*t + phase[i])

where ramp(t) = clip(t / RAMP_IN_S, 0, 1) softly grows the amplitude during
the first few seconds to avoid any sudden jolt at the start of oscillation.
"""
import logging
import os
import signal
import threading
import time
from pathlib import Path
from typing import Optional

import foxglove
import numpy as np
from foxglove.channels import FrameTransformsChannel
from foxglove.messages import (
    FrameTransform,
    FrameTransforms,
    Quaternion,
    Timestamp,
    Vector3,
)
from scipy.spatial.transform import Rotation as R
from yourdfpy import URDF

from oak_streamer import OakStreamer, OakStreamerConfig, is_depthai_available
from reBotArm_control_py.actuator import RobotArm


# --------------------------------------------------------------------------- #
# Tunable parameters
# --------------------------------------------------------------------------- #

# Home pose (degrees, joint1 .. joint6). Oscillation is centered here.
HOME_POSE_DEG = np.array(
    [-8.21, -39.40, -60.14, 21.41, 0.89, 91.72],
    dtype=np.float64,
)
HOME_POSE_RAD = np.deg2rad(HOME_POSE_DEG)

# Sinusoid parameters
PERIOD_S = 20.0                # Full sine period (seconds); larger = slower
FREQ_HZ = 1.0 / PERIOD_S

# Per-joint swing amplitude (degrees); joint 1 is largest but still small
AMPLITUDE_DEG = np.array([15.0, 5.0, 5.0, 8.0, 8.0, 10.0], dtype=np.float64)

# Per-joint phase offset (radians); staggered to produce a wave-like motion
PHASE_RAD = np.array([0.00, 0.25, 0.50, 0.75, 1.00, 1.25], dtype=np.float64) * np.pi

# Oscillation center (radians) -- matches the home pose
CENTER_RAD = HOME_POSE_RAD.copy()

# Velocity caps
VLIM_RAD_S = 0.30              # Demo (oscillation) speed cap, ~17 deg/s
HOMING_VLIM_RAD_S = 0.15       # Homing speed cap, ~8.6 deg/s (slower for safety)

# Soft-start of sinusoidal amplitude after homing completes
RAMP_IN_S = 5.0

# Homing convergence criteria
HOMING_TOLERANCE_RAD = 0.02    # ~1.15 deg per joint
HOMING_TIMEOUT_S = 30.0
HOMING_PROGRESS_EVERY_S = 1.0  # progress print cadence during homing

# Safe-return-to-start (on shutdown / exception) parameters
RETURN_TIMEOUT_S = 12.0        # max time to drive back to the captured start pose
RETURN_SETTLE_S = 1.0          # extra settle time after convergence before disconnect
RETURN_TOLERANCE_RAD = 0.02    # convergence threshold for the return move
RETURN_PROGRESS_EVERY_S = 1.0  # progress print cadence during safe-return
RETURN_TICK_HZ = 50.0          # rate at which we re-issue pos_vel during safe-return

# Shutdown watchdog: upper bound on how long graceful shutdown is allowed to
# run before we hard-exit the process. This is the safety net that guarantees
# the script can be killed even if motorbridge is wedged inside a C-level
# serial read where Python signal handlers can't preempt.
SHUTDOWN_TIMEOUT_S = RETURN_TIMEOUT_S + RETURN_SETTLE_S + 8.0   # ~21 s
# After the operator hits Ctrl+C a second time (`_force_stop`) we abandon the
# safe-return phase entirely and only allow this much time for disconnect.
FORCE_STOP_GRACE_S = 4.0

# Oscillation telemetry cadence (every N control ticks; 500 Hz -> ~10 Hz)
PRINT_EVERY = 50

# Foxglove publishing parameters
URDF_ROOT = Path(__file__).resolve().parent / "urdf"
URDF_PACKAGE = "reBot-DevArm_description_fixend"
URDF_REL_PATH = f"{URDF_PACKAGE}/urdf/reBot-DevArm_fixend.urdf"
URDF_PACKAGE_URI = f"package://{URDF_REL_PATH}"
WORLD_FRAME_ID = "world"
BASE_FRAME_ID = "base_link"
TF_PUBLISH_HZ = 30.0
_TF_PUBLISH_INTERVAL = 1.0 / TF_PUBLISH_HZ

# OAK-4 camera streaming. When True, the demo also spins up an `OakStreamer`
# that publishes a colored point cloud + device static TF onto the same /tf
# topic used by the URDF visualization. The streamer auto-degrades to a no-op
# if depthai is not installed or no OAK device is attached, so leaving this
# True is safe even without a camera connected.
ENABLE_OAK_STREAMER = True
OAK_TF_PREFIX = "oak"               # depthai-ros style: oak_rgb_camera_optical_frame, etc.
OAK_TF_BASE_FRAME = "oak"           # matches the URDF link bolted on link5
OAK_PCL_TOPIC = "/oak/depth/points"
OAK_RGBD_SIZE: tuple[int, int] = (640, 400)
OAK_RGBD_FPS = 30
OAK_IR_LASER_INTENSITY = 0.7        # 0..1; matches the upstream Luxonis example default


# --------------------------------------------------------------------------- #
# Global control flags / time base
# --------------------------------------------------------------------------- #

_running = True
_force_stop = False            # second Ctrl+C: skip safe-return, disconnect now
_sigint_count = 0
_t0: float = 0.0
_amplitude_rad: np.ndarray = np.deg2rad(AMPLITUDE_DEG)
_vlim_arr: np.ndarray = np.full(6, VLIM_RAD_S, dtype=np.float64)
_homing_vlim_arr: np.ndarray = np.full(6, HOMING_VLIM_RAD_S, dtype=np.float64)
_start_pose_rad: np.ndarray | None = None  # captured once, right after enable

# Foxglove publishing state (populated by setup_foxglove)
_fox_server: Optional[foxglove.WebSocketServer] = None
_tf_channel: Optional[FrameTransformsChannel] = None
_urdf: Optional[URDF] = None
# Mapping from URDF revolute-joint name to index in arm.get_positions().
# Built in setup_foxglove() by walking robot.robot.joints in order.
_urdf_joint_index: dict[str, int] = {}
_last_tf_pub_t: float = 0.0

# OAK streamer instance (created by setup_oak_streamer, joined in main's finally)
_oak_streamer: Optional[OakStreamer] = None


def _signal_handler(signum, frame):
    """Three-tier escape hatch for both SIGINT (Ctrl+C) and SIGTERM (kill <pid>):

    * **1st signal** -> graceful shutdown: stop the demo, safe-return to start,
      then disconnect.
    * **2nd signal** -> ``_force_stop``: skip safe-return, just disconnect.
    * **3rd (or higher) signal** -> immediate ``os._exit(130)`` bypassing
      ``finally`` entirely. Motors stay in their last commanded state, so
      use it only when the script is already wedged.

    A background watchdog thread (see ``_shutdown_watchdog``) also force-exits
    after ``SHUTDOWN_TIMEOUT_S`` (or ``FORCE_STOP_GRACE_S`` once ``_force_stop``
    is set), so the process is guaranteed to die in bounded time even if the
    operator never sends a third signal.
    """
    global _running, _force_stop, _sigint_count
    _sigint_count += 1
    name = signal.Signals(signum).name if isinstance(signum, int) else str(signum)
    if _sigint_count == 1:
        print(
            f"\n[demo_mode] {name} received, will return to starting position. "
            "Send signal again to skip safe-return, or a 3rd time to force-exit."
        )
        _running = False
    elif _sigint_count == 2:
        print(f"\n[demo_mode] 2nd {name}, skipping safe-return (will still disconnect)")
        _force_stop = True
        _running = False
    else:
        print(
            f"\n[demo_mode] 3rd {name}, force-exiting NOW via os._exit(130) "
            "(motors stay in last commanded state)"
        )
        os._exit(130)


def _shutdown_watchdog() -> None:
    """Hard-exit the process if shutdown takes too long.

    Sleeps until any signal arrives, then enforces an absolute deadline on the
    rest of the shutdown sequence. Once ``_force_stop`` flips (second signal),
    the deadline tightens to ``FORCE_STOP_GRACE_S`` so the operator gets a
    fast disconnect even if motorbridge is wedged in a serial read.

    Runs as a daemon so it does not block normal exit.
    """
    while _sigint_count == 0:
        time.sleep(0.2)

    deadline = time.monotonic() + SHUTDOWN_TIMEOUT_S
    tightened = False
    while True:
        now = time.monotonic()
        if _force_stop and not tightened:
            deadline = min(deadline, now + FORCE_STOP_GRACE_S)
            tightened = True
        if now >= deadline:
            remaining_signals = max(0, 3 - _sigint_count)
            print(
                f"\n[watchdog] shutdown exceeded deadline; force-exiting via os._exit(130) "
                f"(would have taken {remaining_signals} more signal(s) to do this manually)"
            )
            os._exit(130)
        time.sleep(0.2)


signal.signal(signal.SIGINT, _signal_handler)
signal.signal(signal.SIGTERM, _signal_handler)
threading.Thread(
    target=_shutdown_watchdog,
    name="rebotarm-shutdown-watchdog",
    daemon=True,
).start()


# --------------------------------------------------------------------------- #
# Foxglove integration: URDF + asset server + /tf publishing
# --------------------------------------------------------------------------- #

def asset_handler(uri: str) -> Optional[bytes]:
    """Resolve ``package://`` URDF + mesh URIs from the bundled ``urdf/`` folder.

    Foxglove's 3D panel issues these requests for the URDF custom layer and
    for any ``<mesh filename="package://...">`` references inside the URDF.
    Paths are constrained to live under ``URDF_ROOT`` to prevent traversal.
    """
    if not uri.startswith("package://"):
        return None
    rel = uri[len("package://") :]
    candidate = (URDF_ROOT / rel).resolve()
    try:
        candidate.relative_to(URDF_ROOT.resolve())
    except ValueError:
        return None
    if not candidate.is_file():
        return None
    try:
        return candidate.read_bytes()
    except OSError:
        return None


def setup_foxglove() -> foxglove.WebSocketServer:
    """Load the URDF, start the Foxglove WS server with the asset handler,
    and create the ``/tf`` channel. Returns the running server handle.
    """
    global _fox_server, _tf_channel, _urdf, _urdf_joint_index

    foxglove.set_log_level(logging.INFO)

    urdf_path = URDF_ROOT / URDF_REL_PATH
    if not urdf_path.is_file():
        raise FileNotFoundError(f"URDF not found at {urdf_path}")
    print(f"[foxglove] loading URDF from {urdf_path}")
    # load_meshes=False keeps yourdfpy from trying to resolve `package://` STL
    # references on disk during URDF parsing; Foxglove fetches them later via
    # asset_handler.
    _urdf = URDF.load(
        str(urdf_path),
        load_meshes=False,
        build_scene_graph=True,
        build_collision_scene_graph=False,
    )

    # Map URDF revolute joints to arm.get_positions() indices in URDF order.
    # The reBotArm URDF has joints "joint1, joint2, join3 (typo), joint4,
    # joint5, joint6" plus a fixed "end_joint"; we only animate the revolute
    # ones, paired positionally with motor channels 0..5 from arm.yaml.
    _urdf_joint_index = {}
    for joint in _urdf.robot.joints:
        if joint.type == "revolute":
            _urdf_joint_index[joint.name] = len(_urdf_joint_index)
    print(
        "[foxglove] URDF revolute joints (in order): "
        + ", ".join(f"{name}->q[{idx}]" for name, idx in _urdf_joint_index.items())
    )

    _tf_channel = FrameTransformsChannel(topic="/tf")
    _fox_server = foxglove.start_server(asset_handler=asset_handler)
    print(f"[foxglove] server: {_fox_server.app_url()}")
    print(
        f"[foxglove] add a 3D panel + URDF custom layer with URL\n"
        f"           {URDF_PACKAGE_URI}\n"
        f"           (the SDK asset handler serves it from {URDF_ROOT})"
    )
    return _fox_server


def setup_oak_streamer() -> None:
    """Start the OAK-4 RGBD point cloud streamer, if enabled.

    The streamer shares the existing ``/tf`` channel created by
    ``setup_foxglove`` so the device's static TF tree (oak_rgb_camera_frame,
    oak_*_camera_optical_frame, oak_imu_frame, ...) gets published alongside
    the URDF transforms on the same topic — they hang off the wrist-mounted
    ``oak`` link defined in the URDF, giving a single consistent TF tree.

    Auto-degrades to a no-op if depthai is missing or no OAK device is
    attached; the demo keeps running regardless.
    """
    global _oak_streamer

    if not ENABLE_OAK_STREAMER:
        print("[oak] ENABLE_OAK_STREAMER=False; skipping OAK point cloud setup")
        return
    if not is_depthai_available():
        print("[oak] depthai package not available; OAK point cloud disabled "
              "(install with: uv add depthai)")
        return

    _oak_streamer = OakStreamer(
        OakStreamerConfig(
            tf_prefix=OAK_TF_PREFIX,
            tf_base_frame=OAK_TF_BASE_FRAME,
            pcl_topic=OAK_PCL_TOPIC,
            rgbd_size=OAK_RGBD_SIZE,
            rgbd_fps=OAK_RGBD_FPS,
            ir_laser_intensity=OAK_IR_LASER_INTENSITY,
            tf_channel=_tf_channel,  # share URDF's /tf topic so trees stay merged
            log_prefix="[oak]",
        )
    )
    _oak_streamer.start()
    print(f"[oak] streamer started (point cloud on {OAK_PCL_TOPIC}, "
          f"static + live TF under '{OAK_TF_BASE_FRAME}' frame)")


def maybe_publish_tf(arm: RobotArm, *, force: bool = False) -> None:
    """Publish ``FrameTransforms`` for every URDF joint at up to ``TF_PUBLISH_HZ``.

    Safe to call from any thread that is allowed to read arm state. Throttles
    publishes so callers can invoke this from tight inner loops without flooding
    Foxglove. Pass ``force=True`` to publish unconditionally (e.g. once at the
    end of a phase).
    """
    global _last_tf_pub_t

    if _tf_channel is None or _urdf is None:
        return

    now = time.perf_counter()
    if not force and (now - _last_tf_pub_t) < _TF_PUBLISH_INTERVAL:
        return
    _last_tf_pub_t = now

    try:
        positions = arm.get_positions()
    except Exception as e:
        print(f"[foxglove] could not read arm positions ({e}); skipping /tf tick")
        return

    cfg: dict[str, float] = {}
    for joint in _urdf.robot.joints:
        idx = _urdf_joint_index.get(joint.name)
        if idx is not None and idx < len(positions):
            cfg[joint.name] = float(positions[idx])
        else:
            cfg[joint.name] = 0.0  # fixed joints / unmapped -> identity
    try:
        _urdf.update_cfg(cfg)
    except Exception as e:
        print(f"[foxglove] URDF FK update failed ({e}); skipping /tf tick")
        return

    # Stamp every transform in this bundle with the same wall-clock time so the
    # TF tree is internally consistent for one Foxglove tick. Foxglove rejects
    # transforms with the default zero timestamp (1970-01-01) as stale, which
    # is what caused the "invalid timestamps" you saw.
    ts = Timestamp.now()
    transforms: list[FrameTransform] = [
        FrameTransform(
            timestamp=ts,
            parent_frame_id=WORLD_FRAME_ID,
            child_frame_id=BASE_FRAME_ID,
            translation=Vector3(x=0.0, y=0.0, z=0.0),
            rotation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
        )
    ]
    for joint in _urdf.robot.joints:
        try:
            T_local = _urdf.get_transform(
                frame_to=joint.child, frame_from=joint.parent
            )
        except Exception:
            continue
        t = T_local[:3, 3]
        q = R.from_matrix(T_local[:3, :3]).as_quat()  # (x, y, z, w)
        transforms.append(
            FrameTransform(
                timestamp=ts,
                parent_frame_id=joint.parent,
                child_frame_id=joint.child,
                translation=Vector3(x=float(t[0]), y=float(t[1]), z=float(t[2])),
                rotation=Quaternion(
                    x=float(q[0]), y=float(q[1]), z=float(q[2]), w=float(q[3])
                ),
            )
        )

    try:
        _tf_channel.log(FrameTransforms(transforms=transforms))
    except Exception as e:
        print(f"[foxglove] /tf publish failed ({e})")


# --------------------------------------------------------------------------- #
# Homing helper
# --------------------------------------------------------------------------- #

def home_arm(arm: RobotArm) -> bool:
    """Move the arm slowly to HOME_POSE_RAD and wait for convergence.

    Returns True if homing converged within HOMING_TIMEOUT_S, False otherwise
    (timeout or user abort via Ctrl+C).
    """
    q_start = arm.get_positions(request=True)
    delta = HOME_POSE_RAD - q_start
    max_delta_deg = float(np.max(np.abs(np.degrees(delta))))

    print("-" * 70)
    print(f"[home] start pose (deg): " + " ".join(f"{v:+7.2f}" for v in np.degrees(q_start)))
    print(f"[home] target pose (deg): " + " ".join(f"{v:+7.2f}" for v in HOME_POSE_DEG))
    print(f"[home] per-joint delta (deg): " + " ".join(f"{v:+7.2f}" for v in np.degrees(delta)))
    print(f"[home] max delta: {max_delta_deg:.2f} deg  "
          f"vlim: {HOMING_VLIM_RAD_S} rad/s "
          f"(~{np.degrees(HOMING_VLIM_RAD_S):.1f} deg/s)")
    print("-" * 70)
    print(">>> Verify the arm can SAFELY move in a straight joint-space line")
    print(">>> from the start pose to the home pose (no self-collision, no")
    print(">>> obstacles). Press Enter to begin homing, Ctrl+C to abort.")
    try:
        input(">>> ")
    except (KeyboardInterrupt, EOFError):
        print("\n[home] aborted by user before homing")
        return False

    if not _running:
        return False

    print("[home] driving to home pose...")
    t_start = time.perf_counter()
    last_print = -HOMING_PROGRESS_EVERY_S   # force immediate first print
    converged = False

    while _running:
        arm.pos_vel(HOME_POSE_RAD, vlim=_homing_vlim_arr)
        q_cur = arm.get_positions(request=True)
        err = HOME_POSE_RAD - q_cur
        max_err = float(np.max(np.abs(err)))
        elapsed = time.perf_counter() - t_start

        if elapsed - last_print >= HOMING_PROGRESS_EVERY_S:
            print(f"  t={elapsed:5.1f}s  max_err={np.degrees(max_err):6.2f} deg  "
                  f"q(deg)=" + " ".join(f"{v:+7.2f}" for v in np.degrees(q_cur)))
            last_print = elapsed

        maybe_publish_tf(arm)

        if max_err < HOMING_TOLERANCE_RAD:
            converged = True
            print(f"[home] converged in {elapsed:.1f}s "
                  f"(max_err={np.degrees(max_err):.3f} deg)")
            break

        if elapsed > HOMING_TIMEOUT_S:
            print(f"[home] TIMEOUT after {HOMING_TIMEOUT_S:.0f}s "
                  f"(max_err={np.degrees(max_err):.2f} deg)")
            break

        time.sleep(0.05)

    return converged and _running


# --------------------------------------------------------------------------- #
# Safe return to the captured starting pose (run on shutdown / exception)
# --------------------------------------------------------------------------- #

def return_to_start(
    arm: RobotArm,
    target_rad: np.ndarray,
    timeout_s: float = RETURN_TIMEOUT_S,
) -> bool:
    """Drive the arm back to ``target_rad`` and wait briefly for convergence.

    Runs synchronously on the calling thread, after stopping any active control
    loop so we don't race the demo controller on the bus. A second Ctrl+C
    (``_force_stop``) aborts the safe-return immediately. Always returns
    instead of raising; the caller is responsible for the subsequent disconnect.

    Returns True if the arm converged within tolerance, False on timeout / abort.
    """
    try:
        arm.stop_control_loop()
    except Exception as e:
        print(f"[return] stop_control_loop failed (continuing): {e}")

    if _force_stop:
        print("[return] skipped: force-stop requested")
        return False

    try:
        q_cur = arm.get_positions(request=True)
        delta_deg = np.degrees(target_rad - q_cur)
        print(
            "[return] driving back to start pose; per-joint delta(deg)="
            + " ".join(f"{v:+6.2f}" for v in delta_deg)
        )
    except Exception as e:
        print(f"[return] could not read current pose ({e}); attempting blind drive")

    tick_dt = 1.0 / RETURN_TICK_HZ
    t_start = time.perf_counter()
    last_print = -RETURN_PROGRESS_EVERY_S  # force immediate first print
    converged_at = -1.0
    max_err = float("inf")

    while not _force_stop:
        elapsed = time.perf_counter() - t_start
        if elapsed > timeout_s:
            print(
                f"[return] TIMEOUT after {timeout_s:.1f}s "
                f"(max_err={np.degrees(max_err):.2f} deg); proceeding to disconnect"
            )
            return False

        try:
            arm.pos_vel(target_rad, vlim=_homing_vlim_arr)
        except Exception as e:
            print(f"[return] pos_vel send failed ({e}); proceeding to disconnect")
            return False

        try:
            q_cur = arm.get_positions(request=True)
            max_err = float(np.max(np.abs(target_rad - q_cur)))
        except Exception:
            max_err = float("inf")

        if elapsed - last_print >= RETURN_PROGRESS_EVERY_S:
            print(f"  [return] t={elapsed:4.1f}s  max_err={np.degrees(max_err):6.2f} deg")
            last_print = elapsed

        maybe_publish_tf(arm)

        if max_err < RETURN_TOLERANCE_RAD:
            if converged_at < 0.0:
                converged_at = elapsed
                print(
                    f"[return] reached start pose in {elapsed:.1f}s "
                    f"(max_err={np.degrees(max_err):.3f} deg); settling "
                    f"for {RETURN_SETTLE_S:.1f}s"
                )
            if elapsed - converged_at >= RETURN_SETTLE_S:
                return True

        time.sleep(tick_dt)

    print("[return] aborted by second Ctrl+C")
    return False


# --------------------------------------------------------------------------- #
# Oscillation callback (invoked by RobotArm's control loop at 500 Hz)
# --------------------------------------------------------------------------- #

def demo_controller(arm: RobotArm, dt: float) -> None:
    """Generate slow sinusoidal targets centered on HOME_POSE_RAD."""
    t = time.perf_counter() - _t0

    # Soft-start: ramp amplitude linearly from 0 to 1 over the first RAMP_IN_S seconds
    ramp = min(max(t / RAMP_IN_S, 0.0), 1.0)

    phase_t = 2.0 * np.pi * FREQ_HZ * t + PHASE_RAD
    q_target = CENTER_RAD + ramp * _amplitude_rad * np.sin(phase_t)

    arm.pos_vel(q_target, vlim=_vlim_arr)

    demo_controller._counter += 1
    if demo_controller._counter % PRINT_EVERY == 0:
        q_cur = arm.get_positions()
        tgt_deg = np.degrees(q_target)
        cur_deg = np.degrees(q_cur)
        print(
            f"[{demo_controller._counter:5d}] t={t:6.2f}s ramp={ramp:.2f}  "
            f"tgt(deg)=" + " ".join(f"{v:+6.2f}" for v in tgt_deg) + "  "
            f"cur(deg)=" + " ".join(f"{v:+6.2f}" for v in cur_deg)
        )


demo_controller._counter = 0


# --------------------------------------------------------------------------- #
# Main program
# --------------------------------------------------------------------------- #

def main() -> None:
    global _t0, _start_pose_rad

    print("=" * 70)
    print("  reBotArm demo mode (slow sinusoidal sway)")
    print(f"  home pose (deg): " + " ".join(f"{v:+7.2f}" for v in HOME_POSE_DEG))
    print(f"  period: {PERIOD_S:.1f}s  amplitude(deg): {AMPLITUDE_DEG.tolist()}")
    print(f"  homing vlim: {HOMING_VLIM_RAD_S:.2f} rad/s  "
          f"demo vlim: {VLIM_RAD_S:.2f} rad/s  soft-start: {RAMP_IN_S:.1f}s")
    print("  expected behavior: home first, then 6 joints sway slowly with")
    print("                     phase-shifted sines around the home pose")
    print("  on shutdown / Ctrl+C / SIGTERM / exception: drive back to the pose")
    print("  the arm was in at program start, then disconnect.")
    print("  Escape hatches:")
    print("    1x Ctrl+C  -> graceful safe-return + disconnect")
    print("    2x Ctrl+C  -> skip safe-return, disconnect immediately")
    print(f"    3x Ctrl+C  -> os._exit(130) (motors stay put; instant kill)")
    print(f"    watchdog   -> auto force-exit after ~{SHUTDOWN_TIMEOUT_S:.0f}s "
          f"(or ~{FORCE_STOP_GRACE_S:.0f}s after the 2nd press)")
    print("=" * 70)

    arm: RobotArm | None = None

    setup_foxglove()
    setup_oak_streamer()

    try:
        arm = RobotArm()
        arm.connect()
        print("\n[connect] OK")

        arm.enable()
        print("[enable] OK")

        # Capture the pose the arm was in at program start. We will try to
        # drive back to this pose on shutdown / exception so we leave the
        # arm where we found it.
        _start_pose_rad = arm.get_positions(request=True)
        print(
            "[start_pose] captured (deg): "
            + " ".join(f"{v:+7.2f}" for v in np.degrees(_start_pose_rad))
        )

        arm.mode_pos_vel(vlim=_homing_vlim_arr)
        print(f"[POS_VEL] OK  @ {arm._rate} Hz  (homing speed)")

        if not home_arm(arm):
            print("[abort] homing did not complete")
            return

        print("-" * 70)
        print("[oscillate] starting sinusoidal motion (Ctrl+C to stop)")
        print("-" * 70)

        _t0 = time.perf_counter()
        arm.start_control_loop(demo_controller)

        while _running:
            if not arm.control_loop_active:
                print("[error] control loop stopped unexpectedly; "
                      "treating as fault and returning to start")
                break
            maybe_publish_tf(arm)
            time.sleep(0.05)
    except Exception as e:
        # Re-raised after the finally block so the caller sees the traceback,
        # but log it here so the order of events stays readable on the console.
        print(f"\n[error] {type(e).__name__}: {e}")
        raise
    finally:
        # Stop the OAK streamer first so its worker thread is no longer
        # holding the USB device / publishing into a teardown-in-progress
        # server. It runs on its own thread so this does not block the
        # arm safe-return below.
        if _oak_streamer is not None:
            try:
                _oak_streamer.stop(timeout=3.0)
            except Exception as e:
                print(f"[oak] streamer stop error: {e}")

        if arm is not None:
            if _start_pose_rad is not None:
                print("\n[stop] returning arm to starting position...")
                try:
                    return_to_start(arm, _start_pose_rad)
                except Exception as e:
                    print(f"[return] unexpected error during safe-return: {e}")
            else:
                print("\n[stop] no captured start pose; skipping safe-return")
            print("[stop] disconnecting...")
            try:
                arm.disconnect()
            except Exception as e:
                print(f"[disconnect] error: {e}")
            print("[done] disconnected safely")

        if _fox_server is not None:
            try:
                _fox_server.stop()
            except Exception as e:
                print(f"[foxglove] server stop error: {e}")


if __name__ == "__main__":
    main()
