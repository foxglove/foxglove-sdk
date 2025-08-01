# RGB Camera Visualization

This example demonstrates how to capture video from a camera and visualize it in Foxglove using the Foxglove SDK. The script supports writing data to MCAP files for later playback and analysis.

## Features

- Real-time camera feed visualization in Foxglove
- Support for USB cameras and video devices
- Configurable camera parameters (resolution, FPS)
- MCAP file recording for data persistence
- Optional preview window for debugging
- Comprehensive command-line interface

## Requirements

- Python 3.7+
- A camera device (USB webcam, built-in camera, etc.)
- Foxglove Studio (for visualization)

## Installation

1. Install the required dependencies:
   ```bash
   pip install -r requirements.txt
   ```

   Or using the project file:
   ```bash
   pip install -e .
   ```

## Usage

### Basic Usage

To start streaming from the default camera (usually `/dev/video0` on Linux):

```bash
python main.py
```

### Advanced Usage

```bash
# Use a specific camera device
python main.py --camera-id 1

# Use a camera device path (Linux)
python main.py --camera-id /dev/video1

# Set custom resolution and FPS
python main.py --width 1280 --height 720 --fps 60

# Write data to MCAP file
python main.py --write-mcap

# Write to a specific MCAP file
python main.py --write-mcap --mcap-path my_recording.mcap

# Show preview window (useful for debugging)
python main.py --show-preview

# Use custom Foxglove topic name
python main.py --topic /my_camera/image
```

### Full Example

```bash
python main.py \
  --camera-id 0 \
  --width 1920 \
  --height 1080 \
  --fps 30 \
  --topic /front_camera/image_raw \
  --write-mcap \
  --mcap-path front_camera_recording.mcap \
  --show-preview
```

## Command Line Arguments

### Camera Configuration
- `--camera-id`: Camera ID or device path (default: "0")
- `--width`: Camera frame width (default: 640)
- `--height`: Camera frame height (default: 480)
- `--fps`: Camera frames per second (default: 30.0)
- `--topic`: Foxglove topic name (default: "/camera/image_raw")

### Output Configuration
- `--write-mcap`: Enable MCAP file writing
- `--mcap-path`: Custom MCAP file path (auto-generated if not specified)

### Display Configuration
- `--show-preview`: Show OpenCV preview window

## Camera ID/Path Examples

### Linux
- `0`, `1`, `2`, etc. - Camera indices (maps to `/dev/video0`, `/dev/video1`, etc.)
- `/dev/video0` - Direct device path
- `/dev/v4l/by-id/usb-...` - Persistent device path

### Windows
- `0`, `1`, `2`, etc. - Camera indices
- DirectShow device name (if supported)

### macOS
- `0`, `1`, `2`, etc. - Camera indices

## Viewing in Foxglove

1. Start the script with your desired parameters
2. Open Foxglove Studio
3. Connect to `ws://localhost:8765` (WebSocket connection)
4. Add an "Image" panel
5. Select your camera topic (e.g., `/camera/image_raw`)

## MCAP File Playback

If you recorded data to an MCAP file, you can play it back in Foxglove:

1. Open Foxglove Studio
2. Open the MCAP file (File → Open local file)
3. Add an "Image" panel and select your camera topic
4. Use the playback controls to navigate through the recording

## Troubleshooting

### Camera Not Found
- Check that your camera is connected and recognized by the system
- On Linux, verify the device exists: `ls /dev/video*`
- Try different camera indices (0, 1, 2, etc.)
- Ensure no other application is using the camera

### Permission Issues (Linux)
- Add your user to the `video` group: `sudo usermod -a -G video $USER`
- Log out and back in for changes to take effect

### Low FPS or Performance Issues
- Reduce resolution (`--width` and `--height`)
- Lower FPS target (`--fps`)
- Close other applications using the camera
- Use a faster computer or USB 3.0 connection

### Import Errors
- Ensure all dependencies are installed: `pip install -r requirements.txt`
- Use a virtual environment to avoid conflicts

## Camera Library Choice

This example uses **OpenCV (cv2)** for camera capture because:

- **Wide compatibility**: Works with most USB cameras and video devices
- **Simple API**: Easy to configure and use
- **Cross-platform**: Supports Linux, Windows, and macOS
- **Mature**: Well-established with extensive documentation
- **Flexible**: Supports various formats and camera controls

## Schema Information

The script uses the Foxglove `RawImage` schema with the following configuration:
- **Encoding**: `rgb8` (8-bit RGB)
- **Data**: Raw pixel data as bytes
- **Step**: Bytes per row (width × 3 for RGB)

This is compatible with standard ROS image messages and can be easily converted to other formats if needed.
