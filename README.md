# Foxglove SDK

The Foxglove SDK allows you to log and visualize multimodal data with [Foxglove](https://foxglove.dev).

The SDK is written in Rust, with bindings and packages available for Python and C++.

For ROS integration, see [ros-foxglove-bridge](https://github.com/foxglove/ros-foxglove-bridge).

- Visualize live data using the Foxglove WebSocket protocol
- Log data to [MCAP](https://mcap.dev/) files for later visualization or analysis
- Leverage built-in [Foxglove schemas](https://docs.foxglove.dev/docs/visualization/message-schemas/introduction) for common visualizations, or your own custom messages using a supported serialization format

Visit [Foxglove Docs](https://docs.foxglove.dev/) to get started.

## Packages

<table>
<thead>
<tr><th>Package</th><th>Version</th><th>Description</th></tr>
</thead>
<tbody>

<tr><td><strong>Python</strong></td><td></td><td></td></tr>
<tr>
<td>

[foxglove-sdk](./python/foxglove-sdk/)

</td>
<td>

[![pypi version](https://shields.io/pypi/v/foxglove-sdk)](https://pypi.org/project/foxglove-sdk/)

</td>
<td>Foxglove SDK for Python</td>
</tr>

<tr><td><strong>Rust</strong></td><td></td><td></td></tr>
<tr>
<td>

[foxglove](./rust/foxglove)

</td>
<td>

[![Rust crate version](https://img.shields.io/crates/v/foxglove)](https://crates.io/crates/foxglove)

</td>
<td>Foxglove SDK for Rust</td>
</tr>

<tr><td><strong>ROS</strong></td><td></td><td></td></tr>
<tr>
<td>

[foxglove_msgs](./ros/src/foxglove_msgs)

</td>
<td>

[![ROS Noetic version](https://img.shields.io/ros/v/noetic/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#noetic)<br/>
[![ROS Humble version](https://img.shields.io/ros/v/humble/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#humble)<br/>
[![ROS Jazzy version](https://img.shields.io/ros/v/jazzy/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#jazzy)<br/>
[![ROS Rolling version](https://img.shields.io/ros/v/rolling/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#rolling)

</td>
<td>Foxglove schemas for ROS</td>
</tr>

<tr><td><strong>TypeScript</strong></td><td></td><td></td></tr>
<tr>
<td>

[@foxglove/schemas](./typescript/schemas)

</td>
<td>

[![npm version](https://img.shields.io/npm/v/@foxglove/schemas)](https://www.npmjs.com/package/@foxglove/schemas)

</td>
<td>Foxglove schemas for TypeScript</td>
</tr>

<tr><td><strong>Other</strong></td><td></td><td></td></tr>
<tr>
<td>

[schemas](./schemas)

</td>
<td></td>
<td>Raw schema definitions for ROS, Protobuf, Flatbuffer, JSON, and OMG IDL</td>
</tr>
</tbody>
</table>

## License

[MIT License](/LICENSE)

## Stay in touch

Join our [Discord community](https://foxglove.dev/chat) to ask questions, share feedback, and stay up to date on what our team is working on.
