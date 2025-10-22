# foxglove_bridge

High performance ROS 2 WebSocket bridge using the Foxglove SDK, written in C++.

> [!NOTE]
> The version of `foxglove_bridge` in this repo does not support ROS 1. If you are a ROS 1 user, you can find the source code for the legacy version of `foxglove_bridge` in the [ROS 1 foxglove_bridge repo](https://github.com/foxglove/ros-foxglove-bridge).

## Motivation

Live debugging of ROS systems has traditionally relied on running ROS tooling such as rviz. This requires either a GUI and connected peripherals on the robot, or replicating the same ROS environment on a network-connected development machine including the same version of ROS, all custom message definitions, etc. To overcome this limitation and allow remote debugging from web tooling or non-ROS systems, rosbridge was developed. However, rosbridge suffers from performance problems with high frequency topics and/or large messages, and the protocol does not support full visibility into ROS systems such as interacting with parameters or seeing the full graph of publishers and subscribers.

The `foxglove_bridge` uses the **Foxglove SDK** (this repo!), a similar protocol to rosbridge but with the ability to support additional schema formats such as ROS 2 `.msg` and ROS 2 `.idl`, parameters, graph introspection, and non-ROS systems. The bridge is written in C++ and designed for high performance with low overhead to minimize the impact to your robot stack.

## Install

### Install using apt

You can install `foxglove_bridge` using `apt` from the official ROS package channels for any currently supported ROS 2 distribution, as well as ROS Rolling.

```bash
sudo apt install ros-$ROS_DISTRO-foxglove-bridge
```

Note that packages coming from the ROS channels are updated less frequently than this repository. For the latest set of features, consider [building from source](#build-from-source) or [installing using Docker](#install-using-docker).

### Install using Docker

Docker images are built and published to our public Docker image registry for your convenience.

> [!NOTE]
> We currently only provide Docker builds targeted to the `linux/amd64` platform.

Images can be pulled with

```bash
docker pull us-central1-docker.pkg.dev/foxglove-images/images/foxglove_bridge:<tag>
```

where `<tag>` takes the form of:

```
ros-<ROS distro>-<package version>
```

For example, if you wanted to pull an image based on ROS Kilted and package version v3.2.1:

```bash
docker pull us-central1-docker.pkg.dev/foxglove-images/images/foxglove_bridge:ros-kilted-v3.2.1
```

### Build from source

#### Getting the sources

Clone this repo from GitHub and `cd` to the local ROS workspace:

```bash
git clone https://github.com/foxglove/foxglove-sdk
cd foxglove-sdk/ros
```

All commands in this README hereafter assume you're in the `/ros` subdirectory relative to the repository's root.

#### Build using your ROS environment

Make sure you have ROS installed and your setup files are sourced. Then build:

```bash
make
```

#### Build using Docker

You can also build the bridge using a ROS environment within a Docker container:

```bash
# You can also append your preferred ROS 2 distro codename (e.g. make docker-build-jazzy). If omitted, ROS 'rolling' is used.
make docker-build
```

The built ROS workspace will be written to `/ros` in your `foxglove_sdk` directory.

## Running the bridge

To run the bridge node, it is recommended to use the provided launch file.

If `foxglove_bridge` was built outside your ROS workspace, you need to source the local setup files so ROS 2 can find it:

```bash
source install/local_setup.bash
```

Once ROS is aware of the `foxglove_bridge` package, you can launch the bridge process:

```bash
ros2 launch foxglove_bridge foxglove_bridge_launch.xml port:=8765
```

You can also add `foxglove_bridge` to an existing launch file:

```xml
<launch>
  <!-- Including in another launch file -->
  <include file="$(find-pkg-share foxglove_bridge)/launch/foxglove_bridge_launch.xml">
    <arg name="port" value="8765"/>
    <!-- ... other arguments ... -->
  </include>
</launch>
```

### Configuration

Parameters are provided to configure the behavior of the bridge. These parameters must be set at initialization through a launch file or the command line, they cannot be modified at runtime.

- **port**: The TCP port to bind the WebSocket server to. Must be a valid TCP port number, or 0 to use a random port. Defaults to `8765`.
- **address**: The host address to bind the WebSocket server to. Defaults to `0.0.0.0`, listening on all interfaces by default. Change this to `127.0.0.1` (or `::1` for IPv6) to only accept connections from the local machine.
- **topic_whitelist**: List of regular expressions ([ECMAScript grammar](https://en.cppreference.com/w/cpp/regex/ecmascript)) of whitelisted topic names. Defaults to `[".*"]`.
- **service_whitelist**: List of regular expressions ([ECMAScript grammar](https://en.cppreference.com/w/cpp/regex/ecmascript)) of whitelisted service names. Defaults to `[".*"]`.
- **param_whitelist**: List of regular expressions ([ECMAScript grammar](https://en.cppreference.com/w/cpp/regex/ecmascript)) of whitelisted parameter names. Defaults to `[".*"]`.
- **client_topic_whitelist**: List of regular expressions ([ECMAScript grammar](https://en.cppreference.com/w/cpp/regex/ecmascript)) of whitelisted client-published topic names. Defaults to `[".*"]`.
- **capabilities**: List of supported [server capabilities](https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md). Defaults to `[clientPublish,parameters,parametersSubscribe,services,connectionGraph,assets]`.
- **asset_uri_allowlist**: List of regular expressions ([ECMAScript grammar](https://en.cppreference.com/w/cpp/regex/ecmascript)) of allowed asset URIs. Uses the [resource_retriever](https://index.ros.org/p/resource_retriever/github-ros-resource_retriever) to resolve `package://`, `file://` or `http(s)://` URIs. Note that this list should be carefully configured such that no confidential files are accidentally exposed over the websocket connection. As an extra security measure, URIs containing two consecutive dots (`..`) are disallowed as they could be used to construct URIs that would allow retrieval of confidential files if the allowlist is not configured strict enough (e.g. `package://<pkg_name>/../../../secret.txt`). Defaults to `["^package://(?:[-\w%]+/)*[-\w%]+\.(?:dae|fbx|glb|gltf|jpeg|jpg|mtl|obj|png|stl|tif|tiff|urdf|webp|xacro)$"]`.
- **num_threads**: The number of threads to use for the ROS node executor. This controls the number of subscriptions that can be processed in parallel. 0 means one thread per CPU core. Defaults to `0`.
- **min_qos_depth**: Minimum depth used for the QoS profile of subscriptions. Defaults to `1`. This is to set a lower limit for a subscriber's QoS depth which is computed by summing up depths of all publishers. See also [#208](https://github.com/foxglove/ros-foxglove-bridge/issues/208).
- **max_qos_depth**: Maximum depth used for the QoS profile of subscriptions. Defaults to `25`.
- **best_effort_qos_topic_whitelist**: List of regular expressions (ECMAScript) for topics that should be forced to use 'best_effort' QoS. Unmatched topics will use 'reliable' QoS if ALL publishers are 'reliable', 'best_effort' if any publishers are 'best_effort'. Defaults to `["(?!)"]` (match nothing).
- **include_hidden**: Include hidden topics and services. Defaults to `false`.
- **disable_load_message**: Do not publish as loaned message when publishing a client message. Defaults to `true`.
- **ignore_unresponsive_param_nodes**: Avoid requesting parameters from previously unresponsive nodes. Defaults to `true`.
- **tls**: Enable TLS/WebSocket Secure (WSS). Defaults to `false`.
- **certfile**: Path to an X.509 certificate used for WSS. **Must be supplied if `tls` is set to `true`.**
- **keyfile**: Path to the PEM-encoded PKCS#8 private key used to generate `certfile`. **Must be supplied if `tls` is set to `true`.**
- **use_sim_time**: Use the `/clock` topic as a time source. Defaults to `false`.

#### Capabilities

The `capabilities` parameter can accept one or more of the following values.

- `clientPublish`: Allow clients to advertise channels to send data messages to the server
- `parameters`: Allow clients to get & set parameters
- `parametersSubscribe`: Allow clients to subscribe to parameter changes
- `services`: Allow clients to call services
- `connectionGraph`: Allow clients to subscribe to updates to the connection graph
- `assets`: Allow clients to fetch assets
- `time`: The server may publish time messages using `broadcastTime`. This can be used to sync frame state in panels like the 3D panel if the server's time disagrees with wall time.

## For developers

### Building with local SDK changes

After making SDK changes, don't forget to re-build the SDK and its C++ library:

```bash
make -C ../cpp       # Build SDK in your local environment
# OR
make -C .. build-cpp # Use Docker for SDK build
```

The build commands above for end users pull a pre-built Foxglove SDK binary release from GitHub and link against it when building. This
is convenient because they don't need to have all of the prerequisites required for building the Foxglove SDK installed
locally, but it also means that local changes you make to the SDK won't be reflected in your `foxglove_bridge` builds.

A `make` flag is provided to customize how `foxglove_bridge` satisfies its Foxglove SDK dependency:

- `USE_LOCAL_SDK`: If `ON`, the build will look for the Foxglove SDK in the local file tree, not artifacts from GitHub. For example, if you've built the C++ SDK by running `make build-cpp` in the root directory, this will hook those built files up to the `foxglove_bridge` build. `OFF` by default.

To rebuild the bridge with your locally re-built Foxglove SDK:

```bash
make USE_LOCAL_SDK=ON
```

If you want to build the bridge using Docker:

```bash
make USE_LOCAL_SDK=ON docker-build

# Or to optionally target a specific ROS distro
# make USE_LOCAL_SDK=ON docker-build-<distro codename>
```

### Running tests

`foxglove_bridge` unit tests can be run after `foxglove_bridge` is built:

```bash
make test
```

Tests can also be run under Docker (again, assuming you've already followed the instructions above to build the bridge):

```bash
make docker-test
# Or to optionally target a specific ROS distro
# make USE_LOCAL_SDK=ON docker-test-<distro codename>
```

## Clients

[Foxglove](https://foxglove.dev/) connects to foxglove_bridge for live robotics visualization.

## License

`foxglove_bridge` is released with a MIT license. For full terms and conditions, see the [LICENSE](../../../LICENSE) file.
