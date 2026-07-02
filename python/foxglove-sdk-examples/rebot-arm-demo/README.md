# Visualizing a reBot Arm in Foxglove

This tutorial shows how to visualize a real 6-DOF robot arm — the [reBot Arm B601](https://github.com/reBOT-Robotics) — live in [Foxglove](https://foxglove.dev) using the Foxglove SDK, while the arm runs a gentle demo motion:

| Topic | Schema | Contents |
|-------|--------|----------|
| `/tf` | `foxglove.FrameTransforms` | Forward kinematics of every URDF joint at ~30 Hz — animates the 3D robot model |
| `/joint_states` | `foxglove.JointStates` | Position (rad), velocity (rad/s), and effort (Nm) per joint at up to 50 Hz — for Plot panels |

The robot's URDF and STL meshes are bundled in this folder and served to Foxglove through the SDK's **asset handler**, so a single Python script ([`main.py`](./main.py)) gives you a fully articulated 3D robot view with no ROS and no separate file server.

The demo motion has two phases:

1. **Homing** — slowly drive every joint from its current pose to a fixed home pose. Because this moves in a straight joint-space line, the script asks you to confirm the move is safe before it starts.
2. **Sway** — a low-amplitude, low-frequency sinusoid on all six joints, centered on the home pose, with staggered phases so the arm moves in a smooth wave:

   ```
   q_target[i] = home[i] + ramp(t) * amplitude[i] * sin(2*pi*f*t + phase[i])
   ```

   `ramp(t)` grows from 0 to 1 over the first few seconds so motion starts gently.

## Prerequisites

- A reBot Arm B601 connected and powered.
- A checkout of [`reBotArm_control_py`](https://github.com/reBOT-Robotics) (the arm's Python control library) cloned **next to** this repository:

  ```
  projects/
  ├── foxglove-sdk/
  │   └── python/foxglove-sdk-examples/rebot-arm-demo/   ← you are here
  └── reBotArm_control_py/
  ```

  The example depends on it as an editable path source; if your layout differs, update the `path = ...` line under `[tool.uv.sources]` in [`pyproject.toml`](./pyproject.toml).
- [uv](https://docs.astral.sh/uv/) to run the example.

## Run it

```bash
cd python/foxglove-sdk-examples/rebot-arm-demo
uv run python main.py
```

The script starts the Foxglove server immediately, then connects to the arm, captures its **start pose**, prints the planned homing move, and waits for you to press **Enter**. Once homed, the sway runs until you press **Ctrl+C**.

### View in Foxglove

1. Open [Foxglove](https://app.foxglove.dev) and connect to `ws://localhost:8765` (the script prints a direct link).
2. Import the bundled layout: layout dropdown → *Import from file…* → [`layouts/rebotarm_layout.json`](./layouts/rebotarm_layout.json). It pre-configures a 3D panel with a grid and a URDF custom layer.
3. Or wire it up manually: add a **3D** panel, then a **Custom Layer → URDF** with source *URL* and

   ```
   package://reBot-DevArm_description_fixend/urdf/reBot-DevArm_fixend.urdf
   ```

   Foxglove fetches the URDF and every mesh it references through the SDK's asset handler.
4. Add a **Plot** panel with message paths like `/joint_states.joints[:].velocity` (all six joints) or `/joint_states.joints[0].position` (one joint).

As soon as the first `/tf` message arrives you'll see all six links articulate in real time.

## How it works

### 1. Serve the robot model with an asset handler

Foxglove's URDF layer requests `package://...` URIs — the URDF itself, then each `<mesh filename="package://...">` inside it. The SDK lets you answer those requests with a plain Python callback:

```python
def asset_handler(uri: str) -> bytes | None:
    # resolve package://reBot-DevArm_description_fixend/... under ./urdf
    ...

server = foxglove.start_server(asset_handler=asset_handler)
```

The handler resolves each URI beneath the bundled [`urdf/`](./urdf) folder (with a path-traversal guard) and returns the file bytes. That's the entire "file server".

### 2. Publish the arm's state

The script loads the same URDF with [yourdfpy](https://github.com/clemense/yourdfpy) (`load_meshes=False` — only the kinematic tree is needed) and, on every tick:

- reads the live joint positions off the motor bus,
- runs forward kinematics to get each joint's parent→child transform,
- publishes them all as one `FrameTransforms` bundle on `/tf`, sharing a single timestamp so the tree is consistent for that tick.

Joint telemetry comes from one bus poll (`arm.get_state()`) that returns position, velocity, and torque sampled at the same instant, published as a `JointStates` message — so plotted signals line up with each other and with the 3D view.

One detail worth copying: the URDF's revolute joints are mapped **positionally** to motor indices (the reBot URDF names them `joint1`, `joint2`, `join3` — a typo in the upstream SolidWorks export — `joint4`, `joint5`, `joint6`).

### 3. Drive the arm — and always put it back

The sway controller runs as a callback on the arm's own high-rate control loop; the main thread only watches for faults and keeps Foxglove updated. Everything the script does to the arm is wrapped so that on **any** exit — Ctrl+C, `SIGTERM`, a crash, the control loop dying — it drives the arm back to the pose it captured at startup before disconnecting.

Signals escalate, in case something is stuck:

| Signal # | Behavior |
|---|---|
| 1st | Graceful: stop the sway, return to the start pose, disconnect. |
| 2nd | Skip the return move; disconnect immediately (motors go limp). |
| 3rd | Exit the process instantly; motors hold their last command. |

## Tuning

The tunables live at the top of [`main.py`](./main.py): `HOME_POSE_DEG`, `PERIOD_S`, `AMPLITUDE_RAD`, `PHASE_RAD`, `RAMP_IN_S`, the velocity caps (`SWAY_VLIM`, `SLOW_VLIM`), convergence settings (`TOLERANCE_RAD`, `MOVE_TIMEOUT_S`), and the publish rates (`TF_HZ`, `JOINT_STATES_HZ`).

## Safety

Always be ready to press the e-stop. Homing and the shutdown return move both travel in straight joint-space lines, which can sweep through unsafe poses in a cluttered workspace — verify the printed move before pressing Enter, and keep the start pose clear of obstacles. A second Ctrl+C skips the return move (the arm goes limp where it is, so support it manually); a third exits with the motors still holding position.

## Going further

- Mount a depth camera on the wrist and merge its transforms into the same `/tf` tree, so live point clouds line up with the arm model.
- Record a session with `foxglove.open_mcap(...)` and replay it in Foxglove without the hardware.
- See the [Foxglove SDK docs](https://docs.foxglove.dev/docs/sdk/introduction) for the full API.

**Note:** the repo's `yarn run-python-sdk-examples` CI script skips this folder because it requires a physical reBot Arm and the `reBotArm_control_py` sibling checkout.
