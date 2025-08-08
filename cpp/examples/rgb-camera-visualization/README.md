# RGB Camera Visualization Example

## Installing dependencies

*Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install libopencv-dev
```

**macOS (using Homebrew):**
```bash
brew install opencv
```

**Windows (using vcpkg):**
```bash
vcpkg install opencv
```

## Build

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

1. Open Foxglove (web app or desktop)
2. Connect to `ws://localhost:8765`
3. Add a "Raw Image" panel
4. Select the `/camera/image` topic
5. You should see the live camera feed
