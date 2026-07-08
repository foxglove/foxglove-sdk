> **Use LeRobot's native Foxglove integration instead.** As of LeRobot 0.6.x, pass `--display_mode=foxglove` to `lerobot-teleoperate`, `lerobot-record`, or `lerobot-rollout`, and use `lerobot-dataset-viz --display-mode foxglove` for seekable dataset replay — no custom SDK code required. See the [Native Foxglove Visualization in LeRobot](https://foxglove.dev/blog/native-foxglove-visualization-in-lerobot) guide.
>
> **This example remains as a reference** for building custom Foxglove SDK visualizations on top of LeRobot — specifically, a live 3D kinematic model of the arm driven from the URDF.

# SO-101 Visualization

An example from the Foxglove SDK demonstrating real-time 3D visualization of the SO-101 robot arm.

This example connects to a SO-101 Follower arm, reads joint positions, computes forward
kinematics from the robot's URDF, and publishes the resulting frame transforms — along with joint
states and camera feeds — to Foxglove. The example is based on the SO-101 arm, but you should be
able to modify the example to use the SO-100 quite easily.

It publishes to the same topics as LeRobot's native integration (`/observation/state`,
`/observation/images/<camera>`, plus `/tf` for the 3D model), so the included layout works with
either data source.

For dataset replay, use LeRobot directly:

```bash
lerobot-dataset-viz --repo-id lerobot/svla_so101_pickplace --episode-index 0 --display-mode foxglove
```

See the [blog post](https://foxglove.dev/blog/native-foxglove-visualization-in-lerobot) for full details.

## Prepare Dependencies

LeRobot requires Python 3.12+.

```bash
cd python/foxglove-sdk-examples/so101-visualization
uv venv --python 3.12
source .venv/bin/activate
uv sync
```

## Configure the robot and run the code

Configure and [calibrate your SO-101](https://huggingface.co/docs/lerobot/en/so101#calibrate) using LeRobot. Make sure to identify the configuration name, robot port, and camera IDs. Now you are ready to run the code.

### Parameters

- `--robot.port`: The USB port to connect to the SO-101 arm (e.g., `/dev/ttyUSB0`)
- `--robot.id`: Unique identifier for the robot arm
- `--robot.wrist_cam_id`: Camera ID for wrist camera (optional)
- `--robot.env_cam_id`: Camera ID for environment camera (optional)
- `--output.write_mcap`: Write data to MCAP file (optional, default: False)
- `--output.mcap_path`: Path for MCAP output file (optional, default: auto-generated)

### Examples

Basic usage:

```bash
uv run python main.py --robot.port=/dev/ttyUSB0 --robot.id=my_so101_arm
```

With cameras:

```bash
uv run python main.py \
    --robot.port=/dev/ttyUSB0 \
    --robot.id=my_so101_arm \
    --robot.wrist_cam_id=0 \
    --robot.env_cam_id=4
```

With MCAP logging:

```bash
uv run python main.py \
    --robot.port=/dev/ttyUSB0 \
    --robot.id=my_so101_arm \
    --output.write_mcap \
    --output.mcap_path=robot_session.mcap
```

## Setting up Foxglove

1. In Foxglove, select _Open connection_ from the dashboard or left-hand menu.
2. Select _Foxglove WebSocket_ in the _Open a new connection_ dialog, then enter the URL of your SDK server (`ws://localhost:8765` by default).
3. Open the layout included with the example. In the layout dropdown in the application toolbar, select _Import from file..._, and select `foxglove-sdk/python/foxglove-sdk-examples/so101-visualization/foxglove/lerobot_layout_with_tf.json`.

You should now see your robot's data streaming live!
