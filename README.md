# Foxglove SDK

The Foxglove SDK allows you to log and visualize multimodal data with [Foxglove](https://foxglove.dev).

The core SDK is written in Rust, with bindings for Python, and C++. We publish prebuilt libraries and Python wheels, so you don’t need a Rust development environment.

- Stream live data to Foxglove over a local WebSocket
- Log data to [MCAP](https://mcap.dev/) files for visualization or analysis
- Leverage built-in [Foxglove schemas](https://docs.foxglove.dev/docs/sdk/schemas) for common visualizations, or your own custom messages using a supported serialization format
- ROS packages are available for all supported distributions (see our [ROS 2 tutorial](https://docs.foxglove.dev/docs/getting-started/frameworks/ros2))

Visit [Foxglove SDK Docs](https://docs.foxglove.dev/sdk) to get started.

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

<tr><td><strong>C++</strong></td><td></td><td></td></tr>
<tr>
<td>

[foxglove](./cpp)

</td>
<td>

[![Foxglove SDK version](https://img.shields.io/github/v/release/foxglove/foxglove-sdk?filter=sdk%2F*)](https://github.com/foxglove/foxglove-sdk/releases?q=sdk%2F)

</td>
<td>Foxglove SDK for C++</td>
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

[![ROS Humble version](https://img.shields.io/ros/v/humble/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#humble)<br/>
[![ROS Jazzy version](https://img.shields.io/ros/v/jazzy/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#jazzy)<br/>
[![ROS Kilted version](https://img.shields.io/ros/v/kilted/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#kilted)<br/>
[![ROS Rolling version](https://img.shields.io/ros/v/rolling/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#rolling)

</td>
<td>Foxglove schemas for ROS</td>
</tr>
<tr>
<td>

[foxglove_bridge](./ros/src/foxglove_bridge)

</td>
<td>

[![ROS Humble version](https://img.shields.io/ros/v/humble/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#humble)<br/>
[![ROS Jazzy version](https://img.shields.io/ros/v/jazzy/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#jazzy)<br/>
[![ROS Kilted version](https://img.shields.io/ros/v/kilted/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#kilted)<br/>
[![ROS Rolling version](https://img.shields.io/ros/v/rolling/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#rolling)

</td>
<td>Foxglove ROS bridge</td>
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
