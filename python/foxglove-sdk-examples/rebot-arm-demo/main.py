#!/usr/bin/env python3
"""
Visualize a reBot Arm B601 live in Foxglove while it runs a gentle demo motion.

This tutorial example connects to a reBot Arm over the `reBotArm_control_py`
library, drives it through two phases, and streams its state to Foxglove:

  1. Homing: slowly move every joint from wherever it is to a fixed home pose
     (the operator confirms the move is safe before it starts).
  2. Sway: a low-amplitude, phase-shifted sinusoid on all six joints,
     centered on the home pose:

         q_target[i] = home[i] + ramp(t) * amplitude[i] * sin(2*pi*f*t + phase[i])

While the arm moves, the script publishes to Foxglove:

- ``/tf``            ``foxglove.FrameTransforms`` — forward kinematics of every
                     URDF joint, so the 3D panel animates the robot model
- ``/joint_states``  ``foxglove.JointStates`` — position, velocity, and effort
                     for each joint, ready for Plot panels

The URDF and its STL meshes are served straight from this folder through the
Foxglove SDK's asset handler — no separate file server or ROS required.

The code reads in three steps:

1. Serve the robot model: asset handler + WebSocket server.
2. Publish the arm's state: FK -> /tf, bus telemetry -> /joint_states.
3. Drive the arm: home, sway, and always return to the start pose on exit.
"""

from __future__ import annotations

import os
import signal
import time
from pathlib import Path

import foxglove
import numpy as np
from foxglove.channels import FrameTransformsChannel, JointStatesChannel
from foxglove.messages import (
    FrameTransform,
    FrameTransforms,
    JointState,
    JointStates,
    Quaternion,
    Timestamp,
    Vector3,
)
from reBotArm_control_py.actuator import RobotArm
from scipy.spatial.transform import Rotation
from yourdfpy import URDF

# --------------------------------------------------------------------------- #
# Tunables
# --------------------------------------------------------------------------- #

# Home pose (degrees, joint1..joint6). The sway is centered here.
HOME_POSE_DEG = np.array([-8.21, -39.40, -68.0, 21.41, 0.89, 91.72])
HOME_POSE_RAD = np.deg2rad(HOME_POSE_DEG)

# Sway motion: one full sine period every PERIOD_S seconds, per-joint
# amplitude, and staggered phases so the joints move in a wave.
PERIOD_S = 20.0
AMPLITUDE_RAD = np.deg2rad([20.0, 8.0, 5.0, 8.0, 8.0, 10.0])
PHASE_RAD = np.array([0.0, 0.25, 0.5, 0.75, 1.0, 1.25]) * np.pi
RAMP_IN_S = 5.0  # soft-start: grow the amplitude over the first seconds

# Velocity caps (rad/s). Homing and the shutdown return move are slower.
SWAY_VLIM = np.full(6, 0.30)
SLOW_VLIM = np.full(6, 0.15)

# Convergence criteria for homing and for the shutdown return move.
TOLERANCE_RAD = 0.02  # ~1.15 deg per joint
MOVE_TIMEOUT_S = 30.0

# Foxglove publish rates. /joint_states is faster so velocity/effort plots
# look smooth; /tf only needs to look smooth to the eye.
TF_HZ = 30.0
JOINT_STATES_HZ = 50.0

# The URDF references its meshes as package://reBot-DevArm_description_fixend/...
URDF_ROOT = Path(__file__).resolve().parent / "urdf"
URDF_PATH = URDF_ROOT / "reBot-DevArm_description_fixend/urdf/reBot-DevArm_fixend.urdf"
WORLD_FRAME_ID = "world"
BASE_FRAME_ID = "base_link"

# --------------------------------------------------------------------------- #
# Shutdown handling
#
# The arm is real hardware, so Ctrl+C must not just kill the process: the
# script always tries to drive the arm back to the pose it started in. The
# handler escalates with each signal:
#
#   1st Ctrl+C / SIGTERM -> stop the demo, return to start pose, disconnect
#   2nd                  -> skip the return move, disconnect immediately
#   3rd                  -> os._exit: motors hold their last command
# --------------------------------------------------------------------------- #

_running = True
_skip_return = False
_signal_count = 0


def _signal_handler(signum: int, frame: object) -> None:
    global _running, _skip_return, _signal_count
    _signal_count += 1
    if _signal_count == 1:
        print(
            "\n[stop] signal received; will return the arm to its start pose. "
            "Signal again to skip the return move."
        )
        _running = False
    elif _signal_count == 2:
        print("\n[stop] second signal; skipping the return move")
        _skip_return = True
        _running = False
    else:
        print("\n[stop] third signal; exiting immediately (motors hold position)")
        os._exit(130)


signal.signal(signal.SIGINT, _signal_handler)
signal.signal(signal.SIGTERM, _signal_handler)


# --------------------------------------------------------------------------- #
# Step 1 + 2: Foxglove — serve the URDF, publish /tf and /joint_states
# --------------------------------------------------------------------------- #


def asset_handler(uri: str) -> bytes | None:
    """Serve ``package://`` URIs from the bundled ``urdf/`` folder.

    Foxglove's 3D panel requests the URDF itself and every
    ``<mesh filename="package://...">`` it references through this callback.
    Resolved paths are constrained to URDF_ROOT to prevent path traversal.
    """
    if not uri.startswith("package://"):
        return None
    candidate = (URDF_ROOT / uri[len("package://") :]).resolve()
    try:
        candidate.relative_to(URDF_ROOT.resolve())
    except ValueError:
        return None
    return candidate.read_bytes() if candidate.is_file() else None


class ArmPublisher:
    """Publishes the arm's live state to Foxglove.

    Loads the URDF once for forward kinematics (via yourdfpy), then on every
    ``publish()`` call reads the joint positions off the motor bus, recomputes
    each joint's local transform, and logs a ``FrameTransforms`` bundle plus a
    ``JointStates`` message. Both topics are throttled internally, so it is
    safe to call ``publish()`` from tight control loops.
    """

    def __init__(self) -> None:
        # load_meshes=False: yourdfpy only needs the kinematic tree; Foxglove
        # fetches the meshes itself through asset_handler.
        self.urdf = URDF.load(str(URDF_PATH), load_meshes=False)

        # Map URDF revolute joints (in file order) to motor indices 0..5.
        # The reBot URDF names them joint1, joint2, join3 (an upstream typo),
        # joint4, joint5, joint6 — hence a positional mapping, not a name parse.
        self.joint_names = [
            j.name for j in self.urdf.robot.joints if j.type == "revolute"
        ]
        print(f"[foxglove] URDF joints: {', '.join(self.joint_names)}")

        self.tf_channel = FrameTransformsChannel(topic="/tf")
        self.joint_states_channel = JointStatesChannel(topic="/joint_states")
        self._next_tf = 0.0
        self._next_joint_states = 0.0

    def publish(self, arm: RobotArm) -> None:
        now = time.monotonic()
        if now >= self._next_joint_states:
            self._next_joint_states = now + 1.0 / JOINT_STATES_HZ
            self._publish_joint_states(arm)
        if now >= self._next_tf:
            self._next_tf = now + 1.0 / TF_HZ
            self._publish_transforms(arm)

    def _publish_transforms(self, arm: RobotArm) -> None:
        positions = arm.get_positions()
        self.urdf.update_cfg(
            {name: float(positions[i]) for i, name in enumerate(self.joint_names)}
        )

        # One shared timestamp keeps the TF tree consistent for this tick.
        # (Foxglove treats zero/default timestamps as stale and drops them.)
        ts = Timestamp.now()
        transforms = [
            FrameTransform(
                timestamp=ts,
                parent_frame_id=WORLD_FRAME_ID,
                child_frame_id=BASE_FRAME_ID,
                translation=Vector3(x=0.0, y=0.0, z=0.0),
                rotation=Quaternion(x=0.0, y=0.0, z=0.0, w=1.0),
            )
        ]
        for joint in self.urdf.robot.joints:
            matrix = self.urdf.get_transform(
                frame_to=joint.child, frame_from=joint.parent
            )
            t = matrix[:3, 3]
            q = Rotation.from_matrix(matrix[:3, :3]).as_quat()  # (x, y, z, w)
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
        self.tf_channel.log(FrameTransforms(transforms=transforms))

    def _publish_joint_states(self, arm: RobotArm) -> None:
        # One bus poll returns position, velocity, and torque sampled at the
        # same instant, so the three plotted signals stay coherent.
        pos, vel, torque = arm.get_state()
        self.joint_states_channel.log(
            JointStates(
                timestamp=Timestamp.now(),
                joints=[
                    JointState(
                        name=name,
                        position=float(pos[i]),
                        velocity=float(vel[i]),
                        effort=float(torque[i]),
                    )
                    for i, name in enumerate(self.joint_names)
                ],
            )
        )


# --------------------------------------------------------------------------- #
# Step 3: drive the arm
# --------------------------------------------------------------------------- #


def move_to(
    arm: RobotArm,
    publisher: ArmPublisher,
    target_rad: np.ndarray,
    label: str,
    *,
    check_running: bool = True,
) -> bool:
    """Drive the arm to ``target_rad`` at the slow velocity cap and wait for
    convergence, publishing to Foxglove the whole way. Returns True once every
    joint is within TOLERANCE_RAD; False on timeout or (optionally) shutdown.
    """
    start = time.monotonic()
    last_report = 0.0
    while True:
        if (check_running and not _running) or _skip_return:
            print(f"[{label}] aborted by signal")
            return False
        arm.pos_vel(target_rad, vlim=SLOW_VLIM)
        error = target_rad - arm.get_positions(request=True)
        max_error = float(np.max(np.abs(error)))
        publisher.publish(arm)

        if max_error < TOLERANCE_RAD:
            print(f"[{label}] converged (max error {np.degrees(max_error):.2f} deg)")
            return True
        elapsed = time.monotonic() - start
        if elapsed > MOVE_TIMEOUT_S:
            print(
                f"[{label}] timeout after {elapsed:.0f}s "
                f"(max error {np.degrees(max_error):.2f} deg)"
            )
            return False
        if elapsed - last_report >= 1.0:
            print(
                f"[{label}] t={elapsed:4.1f}s  max error {np.degrees(max_error):6.2f} deg"
            )
            last_report = elapsed
        time.sleep(0.05)


def confirm_homing(arm: RobotArm) -> bool:
    """Show the planned homing move and wait for the operator to approve it.

    Homing moves in a straight joint-space line, which can sweep through poses
    that are unsafe in a cluttered workspace — a human must check first.
    """
    start_deg = np.degrees(arm.get_positions(request=True))
    print("-" * 70)
    print("[home] current pose (deg):", " ".join(f"{v:+7.2f}" for v in start_deg))
    print("[home] target pose  (deg):", " ".join(f"{v:+7.2f}" for v in HOME_POSE_DEG))
    print(">>> Verify the arm can safely move between these poses (no")
    print(">>> self-collision, no obstacles). Enter to start, Ctrl+C to abort.")
    try:
        input(">>> ")
    except (KeyboardInterrupt, EOFError):
        return False
    return _running


def sway(arm: RobotArm, publisher: ArmPublisher) -> None:
    """Run the sinusoidal sway until shutdown is requested.

    The controller callback runs on the arm's own high-rate control loop;
    this thread just watches for faults and keeps Foxglove updated.
    """
    t0 = time.perf_counter()

    def controller(arm: RobotArm, dt: float) -> None:
        t = time.perf_counter() - t0
        ramp = min(t / RAMP_IN_S, 1.0)
        phase = 2.0 * np.pi / PERIOD_S * t + PHASE_RAD
        arm.pos_vel(
            HOME_POSE_RAD + ramp * AMPLITUDE_RAD * np.sin(phase), vlim=SWAY_VLIM
        )

    arm.start_control_loop(controller)
    while _running:
        if not arm.control_loop_active:
            print("[sway] control loop stopped unexpectedly; shutting down")
            return
        publisher.publish(arm)
        time.sleep(0.02)


def main() -> None:
    print("=" * 70)
    print("  reBot Arm demo: home, then sway — visualized live in Foxglove")
    print("  Ctrl+C once: stop and return the arm to its starting pose")
    print("=" * 70)

    publisher = ArmPublisher()
    server = foxglove.start_server(asset_handler=asset_handler)
    print(f"[foxglove] server: {server.app_url()}")
    print("[foxglove] import layouts/rebotarm_layout.json, or add a 3D panel")
    print("           with a URDF custom layer pointing at")
    print(
        "           package://reBot-DevArm_description_fixend/urdf/reBot-DevArm_fixend.urdf"
    )

    arm = RobotArm()
    arm.connect()
    arm.enable()

    # Remember where the arm started so we can leave it as we found it.
    start_pose_rad = arm.get_positions(request=True)
    arm.mode_pos_vel(vlim=SLOW_VLIM)

    try:
        if not confirm_homing(arm):
            print("[home] aborted")
            return
        if not move_to(arm, publisher, HOME_POSE_RAD, "home"):
            return
        print("[sway] starting sinusoidal motion (Ctrl+C to stop)")
        sway(arm, publisher)
    finally:
        try:
            arm.stop_control_loop()
        except Exception as e:
            print(f"[stop] stop_control_loop failed (continuing): {e}")
        if _skip_return:
            print("[stop] return move skipped")
        else:
            print("[stop] returning the arm to its start pose…")
            move_to(arm, publisher, start_pose_rad, "return", check_running=False)
        arm.disconnect()
        server.stop()
        print("[done] disconnected")


if __name__ == "__main__":
    main()
