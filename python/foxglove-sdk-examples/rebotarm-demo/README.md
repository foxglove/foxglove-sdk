# reBot Arm — Demo Mode (slow sinusoidal sway)

Example: slowly sway a [reBot Arm B601](https://github.com/reBOT-Robotics) 6-DOF arm around a fixed home pose, using the [`reBotArm_control_py`](../../../../reBotArm_control_py) Python control library.

The example runs in two phases:

1. **Homing** — slowly drives every joint from its current pose to `HOME_POSE_DEG`
   in a straight joint-space line. The operator must press Enter to confirm that
   this move is safe (no self-collision, no obstacles).
2. **Oscillation** — runs a low-amplitude, low-frequency, phase-shifted sinusoid
   on each of the 6 joints, centered on the home pose. Amplitude is soft-ramped
   over the first few seconds so the start of motion is gentle.

Control law during oscillation:

```
q_target[i] = home[i] + ramp(t) * amplitude[i] * sin(2*pi*f*t + phase[i])
```

## Prerequisites

- A reBot Arm B601 connected and powered, with the host able to reach it over
  the bus configured in [`reBotArm_control_py`](../../../../reBotArm_control_py)
  (USB / CAN / serial, depending on your hardware).
- A working sibling checkout at `../../../../reBotArm_control_py` (i.e. the
  `reBotArm_control_py` repo cloned next to `foxglove-sdk`):

  ```
  projects/
  ├── foxglove-sdk/
  │   └── python/foxglove-sdk-examples/rebotarm-demo/   ← you are here
  └── reBotArm_control_py/
  ```

  If your layout is different, update the `path = ...` line in
  [`pyproject.toml`](./pyproject.toml) under `[tool.uv.sources]`.

## Run

This example uses [uv](https://docs.astral.sh/uv/) like the other
`foxglove-sdk-examples`. It depends on `reBotArm_control_py` as an editable
path source, so `uv` will resolve and install it transparently.

```bash
cd foxglove-sdk/python/foxglove-sdk-examples/rebotarm-demo
uv run python main.py
```

The script will:

1. Connect and enable the arm, **capture the pose it is currently in** (the
   "start pose"), then switch into POS_VEL mode at the homing speed.
2. Print the start pose, the target home pose, and the per-joint deltas, then
   wait for you to press **Enter** to begin homing (or **Ctrl+C** to abort).
3. Once converged, start the sinusoidal sway.

On shutdown — whether triggered by **Ctrl+C**, a runtime exception, or the
control loop dying unexpectedly — the script will stop the demo controller,
drive the arm back to the captured start pose (still in POS_VEL at the homing
velocity cap), wait up to `RETURN_TIMEOUT_S` for convergence plus a short
settle, and only then disable + disconnect. Press **Ctrl+C a second time** to
skip the safe-return and disconnect immediately (use this only if the arm is
in an unrecoverable state — the motors will go limp).

## Tuning

All tunables live at the top of [`main.py`](./main.py):

| Constant | Meaning |
|---|---|
| `HOME_POSE_DEG` | Center pose for the oscillation (deg, joint1..joint6). |
| `PERIOD_S` | Full sine period (seconds); larger = slower. |
| `AMPLITUDE_DEG` | Per-joint swing amplitude (deg). |
| `PHASE_RAD` | Per-joint phase offset (rad); staggered for a wave-like motion. |
| `VLIM_RAD_S` | Velocity cap during oscillation (rad/s). |
| `HOMING_VLIM_RAD_S` | Velocity cap during homing (rad/s). |
| `RAMP_IN_S` | Soft-start ramp duration after homing (s). |
| `HOMING_TOLERANCE_RAD` | Per-joint convergence threshold for homing (rad). |
| `HOMING_TIMEOUT_S` | Abort homing after this many seconds without converging. |
| `RETURN_TIMEOUT_S` | Max time spent driving back to the start pose on shutdown (s). |
| `RETURN_SETTLE_S` | Extra settle time after converging back to start before disconnect (s). |
| `RETURN_TOLERANCE_RAD` | Per-joint convergence threshold for the safe-return move (rad). |

## Safety

Always be ready to press the e-stop. The homing phase moves in a straight
joint-space line which can sweep through poses that are not safe in every
workspace; verify the move before you press Enter.

The on-shutdown safe-return *also* moves in a straight joint-space line — from
wherever the arm happens to be at shutdown back to the captured start pose. If
your workspace is cluttered enough that this could collide, either keep the
start pose well clear of obstacles before launching the script, or press
**Ctrl+C twice** to skip the safe-return (the arm will then go limp where it
is, so support it manually).
