# Streaming a Luxonis OAK camera to Foxglove

This tutorial shows how to take a [Luxonis OAK](https://docs.luxonis.com/) depth camera and stream three of its sensors live into [Foxglove](https://foxglove.dev) using the Foxglove SDK:

| Topic | Schema | Contents |
|-------|--------|----------|
| `/oak/rgb/image` | `foxglove.RawImage` | Raw color video (`bgr8`) |
| `/oak/rgb/calibration` | `foxglove.CameraCalibration` | Color-camera intrinsics + distortion |
| `/oak/depth/image` | `foxglove.RawImage` (`16UC1`) | Stereo depth aligned to the color camera (millimeters) |
| `/oak/imu` | JSON (`sensor_msgs`-like) | Accelerometer (m/s²) + gyroscope (rad/s) |
| `/tf` | `foxglove.FrameTransforms` | One static transform that orients the camera frame upright |

Everything runs in a single Python script ([`main.py`](./main.py)) with no ROS required: the camera does the heavy lifting (stereo matching runs on the device), and the Foxglove SDK serves the results over a WebSocket that the Foxglove app connects to directly.

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

- **Image** → topic `/oak/rgb/image`, **Calibration** `/oak/rgb/calibration` — the live color feed, undistorted using the device's factory intrinsics.
- **3D** → set the panel's **display frame** to `oak` so the scene is upright, then:
  - Enable `/oak/rgb/calibration` under **Camera field-of-view** to see the camera frustum.
  - Enable `/oak/rgb/image` to project the live image into the 3D scene at the camera frustum.
  - Enable `/oak/depth/image` and switch its **Render mode** to **Depth map**, then set **RGB topic** to `/oak/rgb/image` — depth + RGB combine into a colored point cloud.
- **Plot** → message path `/oak/imu.linear_acceleration.x` (or any other axis) — IMU readings over time.

Useful flags: `--rgb-width` / `--rgb-height` (default 1280×720), `--fps` (default 30), and `--record out.mcap` to simultaneously record everything to an [MCAP](https://mcap.dev) file you can replay in Foxglove later.

## How it works

The script follows three steps, in the same order as the code.

### 1. Create a Foxglove channel per stream

The Foxglove SDK ships typed channels for its [well-known schemas](https://docs.foxglove.dev/docs/sdk/schemas) — images, calibration, transforms — which the Foxglove app knows how to visualize out of the box:

```python
rgb_channel = RawImageChannel(topic="/oak/rgb/image")
cal_channel = CameraCalibrationChannel(topic="/oak/rgb/calibration")
depth_channel = RawImageChannel(topic="/oak/depth/image")
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
CAM_B (left)  ──┐                 ┌────────────────► host → /oak/depth/image
                ├─► StereoDepth ──┤
CAM_C (right) ──┘  (align CAM_A)  │
IMU ────────────────────────────────────────────────► host → /oak/imu
```

- The **color camera** outputs one NV12 stream; `getCvFrame()` converts each frame to BGR on the host, which maps directly onto `foxglove.RawImage` with `encoding="bgr8"`.
- The **stereo pair** feeds a `StereoDepth` node with `setDepthAlign(CAM_A)` and `setOutputSize(rgb_width, rgb_height)`, so depth is reprojected into the color camera's optical frame at the same resolution. The depth output is published as a 16UC1 `RawImage` (millimeters).
- The **IMU** node batches samples on the device (`setBatchReportThreshold`) so the host isn't flooded with one tiny packet per sample at 100 Hz.
- The **calibration** is read once from the device (`device.readCalibration()`) and republished as a `foxglove.CameraCalibration` on every RGB frame. Foxglove uses it for three things: drawing the camera frustum in the 3D panel, undistorting the live image in the Image panel, and sampling the RGB image to colorize the depth-derived point cloud in the 3D panel.

### 3. Convert packets to Foxglove messages

The main loop polls each output queue with the non-blocking `tryGet()`, so one loop services all streams at their own natural rates. Each DepthAI packet maps to one Foxglove message:

- A color `ImgFrame` becomes a `RawImage`: width, height, `bgr8` encoding, row stride, and the pixel buffer. Each frame also re-stamps and publishes the cached `CameraCalibration` so MCAP playback always finds a recent one nearby.
- A depth `ImgFrame` becomes a 16UC1 `RawImage` whose pixel values are millimeters — exactly what Foxglove's default depth scale (0.001) expects.
- Each IMU packet becomes one JSON message with the accelerometer and gyroscope vectors.

Every message carries the device timestamp converted to a Foxglove `Timestamp`, so all streams stay mutually synchronized in playback and plots.

### Coordinate frames

Camera data lives in an *optical* frame (Z forward, X right, Y down), while Foxglove's 3D scene is Z-up. Because depth is aligned to the color camera on the device, the RGB image, depth image, and calibration all share CAM_A's optical frame, which we stamp as `frame_id="oak_optical"`. The script publishes a single static `FrameTransform` from `oak` (X forward, Z up) to `oak_optical` using the standard ROS optical rotation. Setting the 3D panel's display frame to `oak` is then enough to see the scene upright. A real robot would extend this transform tree — e.g. publishing `base_link → oak` from its own state — without touching the camera code.

### Distortion-model mapping

Foxglove supports a fixed set of distortion models. DepthAI's `Perspective` is OpenCV's 14-parameter rational polynomial; we map it to Foxglove's `rational_polynomial` and keep the first 8 coefficients (`k1..k6, p1, p2`). DepthAI's `Fisheye` maps to Foxglove's `kannala_brandt` with 4 coefficients. Any other DepthAI model is published with `distortion_model=""` and `D=[]`, so Foxglove falls back to using just the pinhole matrix `K` — the frustum and the colored point cloud still work, only the per-pixel undistortion is skipped.

## Going further

- Encode video on the device (`dai.node.VideoEncoder` → `foxglove.CompressedVideo`) to cut bandwidth for remote viewing.
- See the [Foxglove SDK docs](https://docs.foxglove.dev/docs/sdk/introduction) and [DepthAI v3 examples](https://docs.luxonis.com/software-v3/depthai/examples/) for both halves of the integration.

**Note:** the repo's `yarn run-python-sdk-examples` CI script skips this folder because it requires a physical OAK camera.
