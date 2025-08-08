# RGB Camera Visualization

This example demonstrates how to stream RGB camera data to Foxglove using the Rust SDK.

## Installing Dependencies

This example uses OpenCV for camera capture. You'll need to install OpenCV development libraries:

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install libopencv-dev clang libclang-dev
```

**macOS:**
```bash
brew install opencv
```

**Windows:**
Follow the OpenCV installation guide for Windows and ensure the OpenCV environment variables are set.

## Building the RGB Camera Example

Navigate to the `rust` directory in:

```bash
cargo build -p example_rgb_camera_visualization
```

## Running the Example

### Basic usage (default camera):
```bash
cargo run -p example_rgb_camera_visualization
```

### Specify camera ID:
```bash
cargo run -p example_rgb_camera_visualization -- --camera-id 1
```

## Viewing in Foxglove

1. Open Foxglove (web app or desktop)
2. Connect to `ws://localhost:8765`
3. Add a "Raw Image" panel
4. Select the `/camera/image` topic
5. You should see the live camera feed
