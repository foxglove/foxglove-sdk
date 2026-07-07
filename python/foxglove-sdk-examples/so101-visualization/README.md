# SO-101 Visualization

An example from the Foxglove SDK demonstrating real-time 3D visualization of the SO-101 robot arm.

This example connects to a SO-101 Follower arm, reads joint positions, computes forward
kinematics from the robot's URDF, and publishes the resulting frame transforms — along with joint
states and camera feeds — to Foxglove. The example is based on the SO-101 arm, but you should be
able to modify the example to use the SO-100 quite easily.

> [!NOTE]
> As of [LeRobot](https://github.com/huggingface/lerobot) v0.6.0, Foxglove is a natively supported
> visualization backend: pass `--display_mode=foxglove` to `lerobot-record`, `lerobot-teleoperate`,
> or `lerobot-dataset-viz` and connect the Foxglove app to `ws://localhost:8765` to see camera
> feeds and observation/action series — no extra code required. This example builds on top of that:
> it adds what the built-in integration doesn't provide, a live 3D kinematic model of the arm
> driven by the URDF. It publishes to the same topics as LeRobot (`/observation/state`,
> `/observation/images/<camera>`), so the included layout works with either data source.

## Prepare Dependencies

LeRobot requires Python 3.12+. Create a `lerobot` conda environment and install LeRobot with the
Feetech motor and dataset visualization extras:

```bash
conda create -y -n lerobot python=3.12
conda activate lerobot
conda install ffmpeg -c conda-forge
pip install 'lerobot[feetech,dataset_viz]'
```

Now, install dependencies for this example:
```bash
# Make sure you're in the lerobot conda environment
conda activate lerobot

# Install additional dependencies for this example
pip install -r requirements.txt
```

## Configure the robot and run the code

Configure and [calibrate your SO-101](https://huggingface.co/docs/lerobot/en/so101#calibrate) using LeRobot. Make sure to identify the configuration name, robot port, and camera IDs. Now you are ready to run the code.

### Parameters

- `--robot.port`: The USB port to connect to the SO-101 arm (e.g., `/dev/ttyUSB0`)
- `--robot.id`: Unique identifier for the robot arm
- `--robot.wrist_cam_id`: Camera ID for wrist camera (optional, default: 0)
- `--robot.env_cam_id`: Camera ID for environment camera (optional, default: 4)
- `--output.write_mcap`: Write data to MCAP file (optional, default: False)
- `--output.mcap_path`: Path for MCAP output file (optional, default: auto-generated)

### Examples

Basic usage:
```bash
python main.py --robot.port=/dev/ttyUSB0 --robot.id=my_so101_arm
```

With cameras:
```bash
python main.py \
    --robot.port=/dev/ttyUSB0 \
    --robot.id=my_so101_arm \
    --robot.wrist_cam_id=0 \
    --robot.env_cam_id=4
```

With MCAP logging:
```bash
python main.py \
    --robot.port=/dev/ttyUSB0 \
    --robot.id=my_so101_arm \
    --output.write_mcap \
    --output.mcap_path=robot_session.mcap
```

## Setting up Foxglove

1. In Foxglove, select _Open connection_ from the dashboard or left-hand menu.
2. Select _Foxglove WebSocket_ in the _Open a new connection_ dialog, then enter the URL of your SDK server (`ws://localhost:8765` by default).
3. Open the layout included with the example. In the layout dropdown in the application toolbar, select _Import from file..._, and select `foxglove-sdk/python/foxglove-sdk-examples/so101-visualization/foxglove/lerobot_layout.json`.

You should now see your robot's data streaming live!

## Replaying recorded datasets

LeRobot's Foxglove integration also supports seekable playback of recorded datasets, built on the
SDK's `PlaybackControl` capability: the Foxglove app's playback bar drives play/pause/seek/speed,
and frames are read from the dataset on demand, stamped at their original timestamps.

`dataset_playback.py` demonstrates using this programmatically. Serve an episode of a dataset from
the Hugging Face Hub (or one you recorded yourself with `lerobot-record`):

```bash
# A public SO-101 pick-and-place dataset (the default)
python dataset_playback.py --repo-id lerobot/svla_so101_pickplace --episode-index 0

# A local dataset recorded with lerobot-record
python dataset_playback.py --repo-id my_user/my_dataset --root ~/my_dataset --episode-index 2
```

Then connect the Foxglove app to `ws://localhost:8765` as above, and scrub through the episode
using the playback controls. The same functionality is available from the LeRobot CLI:

```bash
lerobot-dataset-viz --repo-id lerobot/svla_so101_pickplace --episode-index 0 --display-mode foxglove
```
