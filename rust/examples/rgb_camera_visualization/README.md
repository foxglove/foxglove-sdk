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

This example is not built by default because of its dependencies, to build it execute in the root of this repository:

```bash
cargo build --manifest-path rust/examples/rgb_camera_visualization/Cargo.toml
```


## Running the Example

### Basic usage (default camera):
```bash
cargo run --manifest-path rust/examples/rgb_camera_visualization/Cargo.toml
```

### Specify camera ID:
```bash
cargo run --manifest-path rust/examples/rgb_camera_visualization/Cargo.toml -- --camera-id 2
```

## Viewing in Foxglove

1. Open Foxglove (web app or desktop)
2. Connect to `ws://localhost:8765`
3. Add a "Raw Image" panel
4. Select the `/camera/image` topic
5. You should see the live camera feed
