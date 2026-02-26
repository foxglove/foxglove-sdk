# Luxonis OAK Camera + IMU Visualization

Stream low-latency video and IMU data from a Luxonis OAK camera to Foxglove Studio.

## Hardware

Requires a connected OAK device with RGB camera (CAM_A) and IMU, for example:

- OAK-D
- OAK-D-Lite
- OAK-D Pro
- Other Luxonis devices with CAM_A and onboard IMU

## Installation

From this directory or the repo root:

```bash
pip install -r requirements.txt
```

Or install manually:

```bash
pip install depthai foxglove-sdk
```

## Running the Example

1. Connect your OAK camera via USB.
2. Start the example:

```bash
python main.py
```

3. Open Foxglove (app or [Foxglove Studio](https://foxglove.dev/)) and connect to the WebSocket URL printed by the script (e.g. `ws://127.0.0.1:8765`).

### Options

- `--resolution 320x240` or `640x400` – Lower resolution reduces latency (default: `640x400`).
- `--host 127.0.0.1` – WebSocket bind address.
- `--port 8765` – WebSocket port.

Example with lowest resolution:

```bash
python main.py --resolution 320x240
```

## Viewing in Foxglove

1. **Video:** Add a **Raw Image** panel and select the topic `/camera/image`.
2. **IMU:** Add a **Plot** panel and subscribe to `/imu`. Add series for `accel_x`, `accel_y`, `accel_z`, `gyro_x`, `gyro_y`, `gyro_z` (or any subset) to view accelerometer and gyroscope curves over time.

## Troubleshooting

- **"No OAK device found"** – Ensure the camera is connected and that `depthai` can see it (e.g. run a standard depthai example first).
- If the host cannot keep up with the camera, use a lower resolution (`--resolution 320x240`) or skip frames in a future version.
