# SO-101 Visualization

An example from the Foxglove SDK demonstrating real-time visualization of the SO-101 robot arm.

This example connects to a SO-101 Follower arm, reads joint positions, and publishes the robot's
configuration and camera feeds to Foxglove for real-time visualization. The example is based on
SO-101 arm, but you should be able to modify the exapmle to use SO-100 example, quite easily.

## Prepare Dependencies

Follow the [LeRobot installation instructions](https://huggingface.co/docs/lerobot/en/installation) to create a `lerobot` conda environment and install it:

```bash
sudo apt-get install cmake build-essential python-dev pkg-config libavformat-dev libavcodec-dev libavdevice-dev libavutil-dev libswscale-dev libswresample-dev libavfilter-dev pkg-config
git clone https://github.com/huggingface/lerobot.git
cd lerobot
conda create -y -n lerobot python=3.10
conda activate lerobot
conda install ffmpeg -c conda-forge
pip install -e .
pip install -e ".[feetech]"
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
