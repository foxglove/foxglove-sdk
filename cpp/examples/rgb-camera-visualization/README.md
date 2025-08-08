# RGB Camera Visualization Example

This example demonstrates how to stream RGB camera data to Foxglove using the C++ SDK.

## Installing Dependencies

This example uses OpenCV for camera capture. You'll need to install OpenCV development libraries:

*Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install libopencv-dev
```

**macOS (using Homebrew):**
```bash
brew install opencv
```

**Windows:**
Follow the OpenCV installation guide for Windows and ensure the OpenCV environment variables are set.

## Building the RGB Camera Example

Navigate to the `cpp` directory in this repository, and build all examples including this one:

```bash
make BUILD_OPENCV_EXAMPLE=ON build
```

## Running the Example:

Navigate to the cpp build directory (`cpp/build`) and run the example_rgb_camera_visualization executable:

### Basic usage (default camera):
```bash
./example_rgb_camera_visualization
```

### Specify camera ID:
```bash
./example_rgb_camera_visualization --camera-id 4
```

## Viewing in Foxglove

1. Open Foxglove (app or desktop)
2. Connect to `ws://localhost:8765`
3. Add a "Raw Image" panel
4. Select the `/camera/image` topic
5. You should see the live camera feed
