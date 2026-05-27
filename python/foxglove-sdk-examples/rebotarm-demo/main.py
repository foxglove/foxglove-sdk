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
import signal
import time

import numpy as np

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

# Oscillation telemetry cadence (every N control ticks; 500 Hz -> ~10 Hz)
PRINT_EVERY = 50


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


def _sigint_handler(signum, frame):
    global _running, _force_stop, _sigint_count
    _sigint_count += 1
    if _sigint_count == 1:
        print(
            "\n[demo_mode] Ctrl+C received, will return to starting position "
            "(press Ctrl+C again to skip safe-return and disconnect immediately)..."
        )
        _running = False
    else:
        print("\n[demo_mode] second Ctrl+C, skipping safe-return")
        _force_stop = True
        _running = False


signal.signal(signal.SIGINT, _sigint_handler)


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
    print("  on shutdown / Ctrl+C / exception: drive back to the pose the arm")
    print("  was in at program start, then disconnect (press Ctrl+C twice to skip)")
    print("=" * 70)

    arm: RobotArm | None = None

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
            time.sleep(0.05)
    except Exception as e:
        # Re-raised after the finally block so the caller sees the traceback,
        # but log it here so the order of events stays readable on the console.
        print(f"\n[error] {type(e).__name__}: {e}")
        raise
    finally:
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


if __name__ == "__main__":
    main()
