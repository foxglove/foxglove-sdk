# RGB Camera Visualization

This example demonstrates how to stream RGB camera data to Foxglove using the Python SDK.

## Running the Example

Navigate to the example directory (`python/foxglove-sdk-examples/rgb-camera-visualization`).

### Basic usage (default camera):
```bash
uv run python main.py
```

### Specify camera ID:
```bash
uv run python main.py --camera-id 4
```

## Viewing in Foxglove

1. Open Foxglove (app or desktop)
2. Connect to `ws://localhost:8765`
3. Add a "Raw Image" panel
4. Select the `/camera/image` topic
5. You should see the live camera feed
