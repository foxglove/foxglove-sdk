# Streaming a Luxonis OAK camera to Foxglove

This tutorial shows how to take a [Luxonis OAK](https://docs.luxonis.com/) depth camera and stream three of its sensors live into [Foxglove](https://foxglove.dev) using the Foxglove SDK:

| Topic | Schema | Contents |
|-------|--------|----------|
| `/oak/rgb/image` | `foxglove.RawImage` | Raw color video (`bgr8`) |
| `/oak/points` | `foxglove.PointCloud` | XYZ point cloud from stereo depth, in meters |
| `/oak/imu` | JSON (`sensor_msgs`-like) | Accelerometer (m/s²) + gyroscope (rad/s) |
| `/tf` | `foxglove.FrameTransforms` | One static transform that orients the point cloud upright |

Everything runs in a single Python script ([`main.py`](./main.py)) with no ROS required: the camera does the heavy lifting (stereo matching and point-cloud generation run on the device), and the Foxglove SDK serves the results over a WebSocket that the Foxglove app connects to directly.

## Prerequisites

- A Luxonis OAK camera with stereo depth and an IMU (tested with the [OAK-4 D](https://docs.luxonis.com/hardware/products/OAK%204%20D%20Pro)) connected over USB 3 or Ethernet.
- Linux with the Luxonis [udev rules](https://docs.luxonis.com/hardware/platform/deploy/usb-deployment-guide/) installed (for USB devices).
- [uv](https://docs.astral.sh/uv/) to run the example. The dependencies — [`foxglove-sdk`](https://pypi.org/project/foxglove-sdk/) and [DepthAI v3](https://docs.luxonis.com/software-v3/) — are declared in `pyproject.toml` and installed automatically.

## Run it

```bash
cd python/foxglove-sdk-examples/oak-camera-streaming
uv run python main.py
```

Then open [Foxglove](https://app.foxglove.dev), choose **Open connection…**, and connect to `ws://localhost:8765` (the script also prints a direct link at startup).

Add these panels:

- **Image** → topic `/oak/rgb/image` — the live color feed.
- **3D** → enable `/oak/points` — the point cloud. Set the panel's **display frame** to `oak` so the cloud appears upright.
- **Plot** → message path `/oak/imu.linear_acceleration.x` (or any other axis) — IMU readings over time.

Useful flags: `--rgb-width` / `--rgb-height` (default 1280×720), `--fps` (default 30), and `--record out.mcap` to simultaneously record everything to an [MCAP](https://mcap.dev) file you can replay in Foxglove later.

## How it works

The script follows three steps, in the same order as the code.

### 1. Create a Foxglove channel per stream

The Foxglove SDK ships typed channels for its [well-known schemas](https://docs.foxglove.dev/docs/sdk/schemas) — images, point clouds, transforms — which the Foxglove app knows how to visualize out of the box:

```python
rgb_channel = RawImageChannel(topic="/oak/rgb/image")
point_cloud_channel = PointCloudChannel(topic="/oak/points")
tf_channel = FrameTransformsChannel(topic="/tf")
```

The IMU has no dedicated Foxglove schema, so it uses a generic JSON channel. Shaping the payload like ROS `sensor_msgs/Imu` (`linear_acceleration`, `angular_velocity`, a stamped header) keeps it compatible with the Plot panel and familiar to ROS users:

```python
imu_channel = Channel(topic="/oak/imu", message_encoding="json", schema=...)
```

Starting the server is one line — every message logged to a channel after this is broadcast to all connected Foxglove clients:

```python
server = foxglove.start_server()
```

### 2. Build the DepthAI pipeline

DepthAI v3 describes the on-device processing as a graph of nodes:

```text
CAM_A (color) ──── NV12 ────────────────────────────► host → /oak/rgb/image
CAM_B (left)  ──┐
                ├─► StereoDepth ─► PointCloud ──────► host → /oak/points
CAM_C (right) ──┘
IMU ────────────────────────────────────────────────► host → /oak/imu
```

- The **color camera** outputs one NV12 stream; `getCvFrame()` converts each frame to BGR on the host, which maps directly onto `foxglove.RawImage` with `encoding="bgr8"`.
- The **stereo pair** feeds a `StereoDepth` node (rectification and left-right check enabled), whose depth output feeds a `PointCloud` node — both running on the camera, so the host receives ready-made XYZ points. We request meters (`dai.LengthUnit.METER`) to match Foxglove's meter-based 3D scene, and the script double-checks the magnitudes once at startup in case the device firmware still reports millimeters.
- The **IMU** node batches samples on the device (`setBatchReportThreshold`) so the host isn't flooded with one tiny packet per sample at 100 Hz.

### 3. Convert packets to Foxglove messages

The main loop polls each output queue with the non-blocking `tryGet()`, so one loop services all three streams at their own natural rates. Each DepthAI packet maps to one Foxglove message:

- A color `ImgFrame` becomes a `RawImage`: width, height, `bgr8` encoding, row stride, and the pixel buffer.
- `PointCloudData` becomes a `PointCloud`: a packed `float32` buffer with three fields (`x`, `y`, `z`) at 12 bytes per point. Non-finite points are filtered out before publishing.
- Each IMU packet becomes one JSON message with the accelerometer and gyroscope vectors.

Every message carries the device timestamp converted to a Foxglove `Timestamp`, so all three streams stay mutually synchronized in playback and plots.

### Coordinate frames

Camera data lives in an *optical* frame (Z forward, X right, Y down), while Foxglove's 3D scene is Z-up. All messages here are stamped `frame_id="oak_optical"`, and the script publishes a single static `FrameTransform` from `oak` (X forward, Z up) to `oak_optical` using the standard ROS optical rotation. Setting the 3D panel's display frame to `oak` is then enough to see the point cloud upright. A real robot would extend this transform tree — e.g. publishing `base_link → oak` from its own state — without touching the camera code.

## Going further

- Add `CameraCalibrationChannel` messages built from `device.readCalibration2()` intrinsics so Foxglove can project the point cloud onto the image panel.
- Encode video on the device (`dai.node.VideoEncoder` → `foxglove.CompressedVideo`) to cut bandwidth for remote viewing.
- See the [Foxglove SDK docs](https://docs.foxglove.dev/docs/sdk/introduction) and [DepthAI v3 examples](https://docs.luxonis.com/software-v3/depthai/examples/) for both halves of the integration.

**Note:** the repo's `yarn run-python-sdk-examples` CI script skips this folder because it requires a physical OAK camera.
