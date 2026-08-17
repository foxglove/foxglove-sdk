# Asset server

This example serves a small URDF robot and its mesh assets from the local directory. It always
starts a WebSocket server and also starts a remote access gateway when the
`FOXGLOVE_DEVICE_TOKEN` environment variable is set. Both transports use the same asset handler.

The example advertises the following topics:

- `/robot_description` (`std_msgs/msg/String`): the bundled URDF XML, published once per second
- `/tf` (`foxglove.FrameTransforms`): positions the URDF's `base_link` frame relative to `world`

The URDF refers to its mesh as `package://demo_robot/meshes/body.obj`. When Foxglove resolves that
URL, the asset request is handled by the callback in `main.py`. The mesh is authored y-up to match
the 3D panel's default "Mesh up axis" setting.

## Usage

This example uses [uv](https://docs.astral.sh/uv/).

```bash
uv run python main.py
```

Connect Foxglove to `ws://localhost:8765`, open a 3D panel, and add a URDF custom layer whose
source is the `/robot_description` topic. Select `world` as the display frame and use transform
control mode.

To serve the same model through remote access as well:

```bash
FOXGLOVE_DEVICE_TOKEN=your-token-here uv run python main.py
```

For more details, see the Foxglove documentation for
[URDF custom layers and `package://` asset resolution](https://docs.foxglove.dev/docs/visualization/panels/3d#urdf-custom-layer).
