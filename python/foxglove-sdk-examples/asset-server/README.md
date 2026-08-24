# Asset server

This example serves a small URDF robot and its mesh assets from the local directory. It always
starts a WebSocket server and also starts a remote access gateway when the `FOXGLOVE_DEVICE_TOKEN`
environment variable is set. Both transports use the same asset handler.

The example serves the bundled URDF XML as the `/robot_description` parameter. It also advertises a
`/tf` topic (`foxglove.FrameTransforms`), which positions the URDF's `base_link` frame relative to
`world`.

The URDF refers to its mesh as `package://demo_robot/meshes/body.obj`. When Foxglove resolves that
URL, the asset request is handled by the callback in `main.py`. The mesh is authored y-up to match
the 3D panel's default "Mesh up axis" setting.

## Usage

This example uses [uv](https://docs.astral.sh/uv/).

```bash
uv run python main.py
```

Connect Foxglove to `ws://localhost:8765` and open a 3D panel to see the robot. You may need to
enable the visibility of `/robot_description` under the Topics section of the 3D panel settings.

To serve the same model through remote access as well:

```bash
FOXGLOVE_DEVICE_TOKEN=your-token-here uv run python main.py
```

For more details, including how to source a URDF from something other than the
`/robot_description` parameter, see the Foxglove documentation for
[URDF custom layers and `package://` asset resolution](https://docs.foxglove.dev/docs/visualization/panels/3d#urdf-custom-layer).
