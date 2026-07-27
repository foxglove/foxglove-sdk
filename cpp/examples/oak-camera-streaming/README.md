# OAK Camera Streaming Example

This example streams data from an OAK camera to Foxglove using the C++ SDK and the DepthAI C++ library. It publishes the same topics as the [Python tutorial](../../../python/foxglove-sdk-examples/oak-camera-streaming/README.md) (`/oak/points`, `/oak/rgb/image`, `/oak/rgb/calibration`, `/oak/imu`, `/tf`).

For an explanation of the pipeline, coordinate frames, distortion mapping, and Foxglove setup, see the [Python README](../../../python/foxglove-sdk-examples/oak-camera-streaming/README.md). Use the shared layout at [`python/foxglove-sdk-examples/oak-camera-streaming/foxglove/oak_layout.json`](../../../python/foxglove-sdk-examples/oak-camera-streaming/foxglove/oak_layout.json) — in Foxglove, open the layout dropdown and choose **Import from file…**.

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

If your system has another `libfoxglove.so` installed, such as from a ROS environment, make sure the dynamic linker uses the SDK libraries from the build directory:

```bash
LD_LIBRARY_PATH="$PWD:$LD_LIBRARY_PATH" ./example_oak_camera_streaming
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

Then open Foxglove and connect to `ws://localhost:8765`.
