# Luxonis OAK-4 → Foxglove (DepthAI v3)

Example: run a **DepthAI v3** pipeline on an **OAK-4** family device (e.g. [OAK-4D Pro](https://docs.luxonis.com/hardware/products/OAK%204%20D%20Pro)) and stream live data to [Foxglove](https://foxglove.dev) over the Foxglove SDK WebSocket server.

## Topics

| Topic | Schema | Notes |
|-------|--------|--------|
| `/oak/rgb/image_raw` | `foxglove.RawImage` | `bgr8` preview |
| `/oak/rgb/video` | `foxglove.CompressedVideo` | H.264 (NV12 → on-device encode) |
| `/oak/depth/image` | `foxglove.RawImage` | `16UC1`, stereo depth (mm) |
| `/oak/rgb/camera_info` | `foxglove.CameraCalibration` | Color intrinsics + `P` for undistort (see `--camera-info-timing`) |
| `/oak/depth/camera_info` | `foxglove.CameraCalibration` | Rectified stereo / depth projection (`stereoRectify` + `P`) |
| `/oak/imu` | JSON (sensor_msgs-like) | Accel m/s², gyro rad/s |
| `/tf` | `foxglove.FrameTransforms` | **Live:** republished each vision tick (RGB, else H.264, else depth) so the TF tree stays current |
| `/tf_static` | `foxglove.FrameTransforms` | **Same tree once at connect:** matches [depthai-ros `TFPublisher`](https://github.com/luxonis/depthai-ros/blob/kilted/depthai_bridge/src/TFPublisher.cpp) — `oak_*_{rgb,left,right}_camera_frame`, fixed `camera_frame`→`camera_optical_frame` rotation, `getImuToCameraExtrinsics` + RDF-style quaternion for `{prefix}_imu_frame` |

### TF frames (default `--tf-prefix oak`, `--tf-base-frame oak`)

Message `frame_id` values align with depthai-ros / `depthai_bridge` naming:

- **RGB:** `oak_rgb_camera_optical_frame`
- **Depth (stereo left):** `oak_left_camera_optical_frame`
- **IMU:** `oak_imu_frame`

Rigid extrinsics use the same **rotation** conversion (`R_spin @ R_lux @ R_spinᵀ`) and **translation** remap (cm→m, optical→ROS RDF axes) as `TFPublisher::quatFromRotM` / `transFromExtr`. EEPROM `cameraData` drives the full chain when available; otherwise a CAM_A / CAM_B / CAM_C fallback is used.

## Prerequisites

- Ubuntu (or Linux) with **USB3**, udev rules, and [DepthAI v3 / `depthai` Python package](https://docs.luxonis.com/software-v3/depthai.md) installed.
- OAK-4 series camera connected.

## Run

This example uses [uv](https://docs.astral.sh/uv/) like the other `foxglove-sdk-examples`.

**CI:** The repo’s `yarn run-python-sdk-examples` script skips this folder because it needs a physical OAK camera.

```bash
cd foxglove-sdk/python/foxglove-sdk-examples/oak-luxonis-4d
uv run python main.py
```

From the monorepo with a local SDK build:

```bash
cd python/foxglove-sdk-examples/oak-luxonis-4d
uv run --with ../../foxglove-sdk main.py
```

Optional flags: `--no-raw-rgb`, `--no-h264`, `--no-depth`, `--no-imu`, `--no-calibration`, **`--no-tf`**, **`--tf-once`** (publish `/tf` only at startup; default is continuous `/tf` with vision timestamps), **`--tf-prefix`** (default `oak`), **`--tf-base-frame`** (default `oak`, rig root for the “extra” camera in EEPROM), `--record PATH.mcap`, `--rgb-width`, `--rgb-height`, `--fps`, `--stereo-width`, `--stereo-height`, IMU tuning (`--imu-max-packets`, `--imu-accel-hz`, `--imu-gyro-hz`, `--imu-batch-threshold`, `--imu-max-batch-reports`), and **`--camera-info-timing`**: `with_images` (default) republishes `/oak/rgb/camera_info` and `/oak/depth/camera_info` **with the same timestamp as each frame** (best for rectification / sync); `once` sends a single latched message at startup only.

Depth uses the **left** stereo optical frame (`oak_left_camera_optical_frame`). `camera_info` for depth is **rectified** (`stereoRectify`); small alignment residuals vs. raw factory B→A are the same class of issue as in ROS if you compare rectified depth to color.

Color uses **one NV12** stream from CAM_A, split to the host (BGR preview) and on-device H.264, matching the official [video_encode](https://raw.githubusercontent.com/luxonis/depthai-core/main/examples/python/VideoEncoder/video_encode.py) pattern. If RGB is still empty, try lower resolution, e.g. `--rgb-width 960 --rgb-height 540`.

## View in Foxglove

1. Open [Foxglove](https://app.foxglove.dev) and connect to `ws://localhost:8765`.
2. Add panels:
   - **Image** → `/oak/rgb/image_raw` (BGR / raw).
   - **Image** → `/oak/depth/image` (set color mode for 16-bit / depth as needed).
   - **Compressed video** → `/oak/rgb/video` (H.264).
   - **Plot** → `/oak/imu` (e.g. `linear_acceleration.x` or `angular_velocity.z` in the message JSON).

3. Optional: **Import layout** from `foxglove/oak4d.json` (toolbar → _Import layout from file…_). It pre-configures **Raw Image** panels for RGB and depth; add a **Compressed video** panel manually for `/oak/rgb/video` and a **Plot** panel for `/oak/imu` if you want those in the same layout.

## References

- [depthai-ros / `depthai_bridge` (TF + URDF)](https://github.com/luxonis/depthai-ros)
- [DepthAI v3 camera examples](https://docs.luxonis.com/software-v3/depthai/examples/#Depthai%20Examples-Camera)
- [Stereo depth](https://docs.luxonis.com/software-v3/depthai/examples/stereo_depth/stereo_depth)
- [IMU](https://docs.luxonis.com/software-v3/depthai/examples/imu/imu_accelerometer_gyroscope)
- [Foxglove SDK](https://docs.foxglove.dev/docs/sdk/introduction)

## Robot arm (later)

Publish your own `FrameTransforms` from the arm base to `{tf_base_frame}` (default `oak`) or to `{tf_prefix}_imu_frame` / optical frames; keep this process focused on the camera. Prefer **Ethernet** on the robot when available.
