# reBot Arm — Demo Mode (slow sinusoidal sway)

Example: slowly sway a [reBot Arm B601](https://github.com/reBOT-Robotics) 6-DOF arm around a fixed home pose, using the [`reBotArm_control_py`](../../../../reBotArm_control_py) Python control library, with a live URDF visualization streamed to [Foxglove](https://foxglove.dev) over a WebSocket server.

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

## Foxglove integration

Throughout all phases (homing, oscillation, return-to-start) the script:

- Starts a Foxglove WebSocket server on `ws://localhost:8765`.
- Registers an `asset_handler` that serves `package://reBot-DevArm_description_fixend/...`
  URIs from the bundled [`urdf/`](./urdf) folder (URDF file + STL meshes).
- Publishes `FrameTransforms` on `/tf` at ~30 Hz using [yourdfpy](https://github.com/clemense/yourdfpy)
  for forward kinematics from the live joint positions read off the motor bus.

The bundled URDF [`reBot-DevArm_fixend.urdf`](./urdf/reBot-DevArm_description_fixend/urdf/reBot-DevArm_fixend.urdf)
defines the six revolute joints (`joint1`, `joint2`, `join3`*, `joint4`, `joint5`,
`joint6`) plus the fixed `end_joint`. They are mapped positionally to the motor
channels declared in [`reBotArm_control_py/config/arm.yaml`](../../../../reBotArm_control_py/config/arm.yaml),
so joint *N* in the URDF tracks motor index *N − 1* from the YAML.

\* yes, `join3` (missing 't') is a typo in the upstream SolidWorks-exported URDF; the script handles it correctly.

### Mounted camera: Luxonis OAK 4 Pro

The URDF also includes a Luxonis OAK-4 family camera bolted above `link5`
(the wrist-pitch link), mirroring the upstream
[`depthai_descriptions/urdf/include/base_macro.urdf.xacro`](https://github.com/luxonis/depthai-ros/blob/kilted/depthai_descriptions/urdf/include/base_macro.urdf.xacro)
frame layout but flattened to plain URDF (no `xacro` toolchain required):

```
link5  [oak_mount_joint]        →  oak                 (mount base frame; tune origin here)
oak    [oak_model_origin_joint] →  oak_model_origin    (visual mesh, +rpy="1.5708 0 1.5708")
```

The mesh
([`urdf/reBot-DevArm_description_fixend/meshes/OAK4-D.stl`](./urdf/reBot-DevArm_description_fixend/meshes/OAK4-D.stl))
and its
[`OAK4-D.LICENSE`](./urdf/reBot-DevArm_description_fixend/meshes/OAK4-D.LICENSE)
are taken from
[luxonis/depthai-ros@kilted](https://github.com/luxonis/depthai-ros/tree/kilted/depthai_descriptions/urdf/models)
under the MIT license and dropped into the same package as the rest of the arm
meshes. The URDF references it by a **relative** path
(`../meshes/OAK4-D.stl`, resolved against the URDF file's own URL) so Foxglove
loads it without any cross-package `package://` lookup — which is what
upstream-style `package://depthai_descriptions/...` URIs require, and which
some Foxglove versions handle inconsistently.

The upstream package does not yet ship a dedicated `OAK4-D-PRO.stl` (see
[issue #772](https://github.com/luxonis/depthai-ros/issues/772)), so we use
the geometrically closest `OAK4-D.stl` — the driver itself does the same
fallback.

To **tune the physical mount**, edit `oak_mount_joint`'s `<origin xyz="..." rpy="..."/>`
in the URDF. The current default is `xyz="0.0320 0.0 -0.037"` (placement on
`link5`) with `rpy="3.14159 0 0"` — a 180° roll around `link5`'s X axis that
flips the camera right-side-up, since `link5`'s local +Z does not point "up"
in the canonical depthai camera frame. If your physical orientation differs,
the URDF comment above the joint lists common alternative `rpy` values
(camera-backward, sky-pointing, etc.). The Foxglove asset handler re-serves
the URDF on every 3D-panel reload, so you'll see the change after
_Custom Layer → URDF → Reload_.

If you're also running a depthai_ros driver / [`oak-luxonis-4d`](../oak-luxonis-4d)
streamer in parallel, it will publish the camera-internal optical frames
(`oak_rgb_camera_optical_frame`, `oak_left_camera_optical_frame`,
`oak_imu_frame`, …) on `/tf` rooted at this `oak` frame, so live camera /
point-cloud topics will line up with the arm visualization automatically.

### Built-in OAK point cloud streamer

This demo also ships its own minimal OAK-4 publisher in
[`oak_streamer.py`](./oak_streamer.py) — a single library file with one
`OakStreamer` class — so you don't need to run `../oak-luxonis-4d` in a
second process to see live depth in Foxglove.

What it does (when an OAK device is attached and `depthai` is installed):

- Opens a DepthAI v3 pipeline (color + stereo + `dai.node.RGBD`) and
  publishes a **colored point cloud** on `/oak/depth/points` (XYZ float32 +
  separate `red` / `green` / `blue` / `alpha` Uint8 fields — Foxglove's
  *"RGBA (separate fields)"* color mode).
- Reads the device calibration and publishes the **depthai-ros-style**
  static TF tree (`oak_{rgb,left,right}_camera_{frame,optical_frame}`,
  `oak_imu_frame`) **rooted at the URDF's `oak` link**, so the camera
  visualization, point cloud, and URDF share one consistent TF tree.
- Re-stamps the TF tree on every received cloud, so Foxglove sees fresh
  timestamps and never marks the device transforms as stale.
- Runs in its own daemon thread with `start()` / `stop()`. `main.py` only
  calls these two methods.
- **Auto-degrades**: if `depthai` is missing or no OAK is plugged in,
  `OakStreamer.start()` logs a warning and exits cleanly — the homing and
  sinusoidal-sway demo keeps running.

Tunables (top of `main.py`):

| Constant                | Meaning                                                                                       |
| ----------------------- | --------------------------------------------------------------------------------------------- |
| `ENABLE_OAK_STREAMER`   | Master switch (`True` by default). Set `False` to disable the camera publisher.               |
| `OAK_TF_PREFIX`         | TF naming prefix; matches depthai-ros (`{prefix}_rgb_camera_optical_frame`, etc.).            |
| `OAK_TF_BASE_FRAME`     | Root frame the device TF tree attaches to. Must match a URDF link (default `oak` on `link5`). |
| `OAK_PCL_TOPIC`         | Foxglove topic for the colored point cloud.                                                   |
| `OAK_RGBD_SIZE`         | Color / stereo / RGBD output resolution (width, height).                                      |
| `OAK_RGBD_FPS`          | Pipeline FPS.                                                                                 |
| `OAK_IR_LASER_INTENSITY`| IR dot-projector intensity, 0..1; matches the Luxonis stereo example default.                 |

In Foxglove: add a **3D panel**, follow frame `world` (or `base_link`),
enable the URDF custom layer (already in [`foxglove/rebotarm_layout.json`](./foxglove/rebotarm_layout.json)),
then add the `/oak/depth/points` PointCloud topic — set its color mode to
*"RGBA (separate fields)"* if it isn't auto-detected.

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

On shutdown — whether triggered by **Ctrl+C**, **`SIGTERM`** (`kill <pid>`),
a runtime exception, or the control loop dying unexpectedly — the script will
stop the demo controller, drive the arm back to the captured start pose (still
in POS_VEL at the homing velocity cap), wait up to `RETURN_TIMEOUT_S` for
convergence plus a short settle, and only then disable + disconnect.

### Escape hatches (in order of escalating aggression)

The script catches `SIGINT` (`Ctrl+C`) and `SIGTERM` (`kill <pid>`) the same way
and tiers its response by repeat count:

| Press / signal #   | Behavior                                                                                                                  |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| **1st**            | Graceful: stop demo, safe-return to start pose, disable, disconnect.                                                      |
| **2nd**            | Skip safe-return; just disable and disconnect (motors go limp where they are).                                            |
| **3rd**            | `os._exit(130)` immediately — bypasses Python's `finally` entirely. Motors stay in their last commanded state. Last resort. |
| **watchdog timer** | If the graceful path hangs (e.g. `motorbridge` is wedged in a serial read), a background daemon thread hard-exits after `SHUTDOWN_TIMEOUT_S` (~21 s), or `FORCE_STOP_GRACE_S` (~4 s) once the 2nd press has flipped the script into force-stop mode. |

This means even if the script appears unresponsive to `Ctrl+C` because the
underlying serial / USB layer is stuck inside a C-level call where Python
signal handlers can't preempt, the watchdog will reap the process in bounded
time — you should never need `kill -9`. If you do, run `pkill -KILL -f rebotarm-demo`
from another shell.

## View in Foxglove

1. Open [Foxglove](https://app.foxglove.dev) and choose _Open connection_ → _Foxglove WebSocket_,
   then enter the URL printed by the SDK (default `ws://localhost:8765`).
2. **Import the bundled layout**: in the layout dropdown, pick _Import from file…_
   and select
   [`foxglove/rebotarm_layout.json`](./foxglove/rebotarm_layout.json). It pre-configures a 3D panel
   following `base_link`, a grid, and a URDF custom layer wired to
   `package://reBot-DevArm_description_fixend/urdf/reBot-DevArm_fixend.urdf` — Foxglove
   pulls both the URDF and every STL mesh through the SDK's `asset_handler`, so no extra
   server setup or file paths are needed on your side.
3. If you'd rather wire it up manually, add a **Custom Layer → URDF** to a 3D panel with:
   - **Source**: URL
   - **URL**: `package://reBot-DevArm_description_fixend/urdf/reBot-DevArm_fixend.urdf`
   - Make sure the panel's **Follow frame** is `base_link` (or `world`).

   As soon as the script publishes the first `/tf` message you should see all six links
   articulate in real time.

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
| `SHUTDOWN_TIMEOUT_S` | Absolute upper bound for graceful shutdown before the watchdog hard-exits (s). |
| `FORCE_STOP_GRACE_S` | Tightened deadline after the 2nd Ctrl+C/SIGTERM, applied to the disconnect phase only (s). |

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

A third Ctrl+C — or the watchdog firing — exits the Python process via
`os._exit(130)` without disabling the motors. They stay holding the last
commanded torque/position, which is usually fine but can surprise you: be
ready to e-stop or kill power if the arm is in an awkward pose.
