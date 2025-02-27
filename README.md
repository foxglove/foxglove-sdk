# Foxglove SDK

This repo provides libraries and schemas to log and visualize multimodal data with [Foxglove](https://foxglove.dev).

Visit [Foxglove Docs](https://docs.foxglove.dev/) to get started.

## Packages

<table>
<thead>
<tr><th>Package</th><th>Version</th><th>Description</th></tr>
</thead>
<tbody>
<tr><td><strong>Python</strong></td><td></td><td></td></tr>
<tr>
<td>foxglove-sdk</td>
<td>

[![pypi version](https://shields.io/pypi/v/foxglove-sdk)](https://pypi.org/project/foxglove-sdk/)

</td>
<td>Python SDK</td>
</tr>
<tr>
<td>foxglove-schemas-flatbuffer</td>
<td>

[![pypi version](https://shields.io/pypi/v/foxglove-schemas-flatbuffer)](https://pypi.org/project/foxglove-schemas-flatbuffer/)

</td>
<td>Precompiled flatbuffers</td>
</tr>
<tr>
<td>foxglove-schemas-protobuf</td>
<td>

[![pypi version](https://shields.io/pypi/v/foxglove-schemas-protobuf)](https://pypi.org/project/foxglove-schemas-protobuf/)

</td>
<td>Precompiled protocol buffers</td>
</tr>
<tr><td><strong>C++</strong></td><td></td><td></td></tr>
<tr>
<td>foxglove-websocket</td>
<td>

[![conan version](https://img.shields.io/conan/v/foxglove-websocket)](https://conan.io/center/recipes/foxglove-websocket)

</td>
<td>

C++ implementation of the WebSocket protocol<br/>
Repo: [foxglove/ws-protocol](https://github.com/foxglove/ws-protocol/tree/main/cpp)

</td>
</tr>
<tr><td><strong>Rust</strong></td><td></td><td></td></tr>
<tr>
<td>foxglove</td>
<td>

[![conan version](https://img.shields.io/crates/v/foxglove)](https://crates.io/crates/foxglove)

</td>
<td>Rust SDK</td>
</tr>
<tr><td><strong>ROS</strong></td><td></td><td></td></tr>
<tr>
<td>foxglove_msgs</td>
<td>

[![ROS Noetic version](https://img.shields.io/ros/v/noetic/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#noetic)<br/>
[![ROS Humble version](https://img.shields.io/ros/v/humble/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#humble)<br/>
[![ROS Jazzy version](https://img.shields.io/ros/v/jazzy/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#jazzy)<br/>
[![ROS Rolling version](https://img.shields.io/ros/v/rolling/foxglove_msgs)](https://index.ros.org/p/foxglove_msgs#rolling)

</td>
<td>Foxglove message definitions for ROS 1 and ROS 2</td>
</tr>
<tr>
<td>foxglove_bridge</td>
<td>

[![ROS Noetic version](https://img.shields.io/ros/v/noetic/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#noetic)<br/>
[![ROS Humble version](https://img.shields.io/ros/v/humble/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#humble)<br/>
[![ROS Jazzy version](https://img.shields.io/ros/v/jazzy/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#jazzy)<br/>
[![ROS Rolling version](https://img.shields.io/ros/v/rolling/foxglove_bridge)](https://index.ros.org/p/foxglove_bridge#rolling)

</td>
<td>

ROS implementation of the WebSocket protocol<br/>
Repo: [foxglove/ros-foxglove-bridge](https://github.com/foxglove/ros-foxglove-bridge)

</td>
</tr>
<tr><td><strong>JavaScript / TypeScript</strong></td><td></td><td></td></tr>
<tr>
<td>@foxglove/schemas</td>
<td>

[![npm version](https://img.shields.io/npm/v/@foxglove/schemas)](https://www.npmjs.com/package/@foxglove/schemas)

</td>
<td>Foxglove schemas for TypeScript</td>
</tr>
<tr>
<td>@foxglove/ws-protocol</td>
<td>

[![npm version](https://img.shields.io/npm/v/@foxglove/ws-protocol)](https://www.npmjs.com/package/@foxglove/ws-protocol)

</td>
<td>

TypeScript implementation of the WebSocket protocol<br/>
Repo: [foxglove/ws-protocol](https://github.com/foxglove/ws-protocol/tree/main/typescript/ws-protocol)

</td>
</tr>
</tbody>
</table>

### Other

The [schemas](./schemas) directory contains type definitions for ROS 1, ROS 2, Protobuf, JSON Schema, TypeScript, and OMG IDL. They can be copied and used in your application directly.

## Stay in touch

Join our [Discord community](https://foxglove.dev/chat) to ask questions, share feedback, and stay up to date on what our team is working on.
