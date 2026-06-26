# OAK Camera Streaming Example

This example streams data from an OAK camera to Foxglove using the C++ SDK and the DepthAI C++ library.

It publishes:

- `/oak/points`: colored point cloud
- `/oak/rgb/image`: RGB camera image
- `/oak/rgb/calibration`: RGB camera calibration
- `/oak/imu`: IMU samples as JSON
- `/tf`: transform from `oak` to `oak_optical`

## Installing Dependencies

Install the [DepthAI C++ library](https://github.com/luxonis/depthai-core) by following the official DepthAI installation documentation. CMake must be able to find the `depthai` package; if DepthAI is installed in a non-standard prefix, pass it to CMake with `CMAKE_PREFIX_PATH` or `depthai_DIR`.

This example is optional. When DepthAI is not found, CMake skips `example_oak_camera_streaming`.

## Building

From the `cpp` directory in this repository:

```bash
make FOXGLOVE_BUILD_EXAMPLES=ON build
```

## Running

From the C++ build directory:

```bash
./example_oak_camera_streaming
```

Useful options:

```bash
./example_oak_camera_streaming --depth-source stereo
./example_oak_camera_streaming --depth-source neural
./example_oak_camera_streaming --port 8765
./example_oak_camera_streaming --record oak.mcap
./example_oak_camera_streaming --point-unit auto
```

`--point-unit auto` is the default. It detects whether DepthAI point coordinates are meter-scale or millimeter-scale before publishing Foxglove point clouds in meters.

## Viewing in Foxglove

Open Foxglove and connect to `ws://localhost:8765`. Add a 3D panel and set the display frame to `oak` to view the point cloud upright. Add an Image panel for `/oak/rgb/image` and select `/oak/rgb/calibration` as the calibration topic.
