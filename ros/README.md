# ROS packages

The goal of the ROS packages here are to provide a simple way for ROS 1/2 users to connect their ROS middleware systems into foxglove, either through the foxglove websocket or through the remote access gateway.

# Architecture

The current architecture here consists of 4 packages:

1. foxglove_msgs - Messages useful for publishing or subscribing from foxglove.  Not directly depended on by any of the other packages.
2. foxglove_bridge_core - Provides common functionality between the ROS 1 and ROS 2 bridges, such as capabilities, logging, utils, types, and a transport manager.  Relatively thin; the bridges themselves are much larger.  Also not expected to be useful outside of ROS, since it kind of assumes a ROS-like system.
3. foxglove_bridge - The ROS 2 bridge.  Depends on foxglove_bridge_core for some common functionality, then implements a whole host of ROS 2-specific things.  Much larger than the foxglove_bridge_core.
4. foxglove_bridge_ros1 - The ROS 1 bridge.  Depends on foxglove_bridge_core for some common functionality, then implements a whole host of ROS 1-specific things.  Much larger than the foxglove_bridge_core.

However, after discussing it with the team, we agreed that this architecture is not the best.
It puts the ROS 2 bridge at risk of regression, for a ROS 1 bridge that is obsolete the day it is shipped (ROS 1 stopped being supported in 2025).

The architectures we considered:
1.  The current one, with a shared layer between ROS 1 and ROS 2.
2.  Parallel implementations of ROS 1 and ROS 2 bridge, accepting the code duplication.
3.  Putting both ROS 1 and ROS 2 bridges into the same package, with conditional compilation as necessary.
4.  Leaving the ROS 2 bridge alone, but splitting the ROS 1 bridge into a common and ROS-1-specific part.

We agreed that, if we are going to go forward with this, we should choose architecture 2.

# ROS 2 Packages

## Building

All build targets run inside Docker containers. Targets without a distribution
suffix default to `rolling`.

Supported distributions: `humble`, `jazzy`, `kilted`, `rolling`.

### Build the Docker image

```sh
make docker-build-image          # rolling
make docker-build-image-humble   # specific distro
```

### Build targets

```sh
make docker-build          # rolling
make docker-build-humble   # specific distro
```

### Run tests

```sh
make docker-test          # rolling
make docker-test-humble   # specific distro
```

### Build .deb packages

Uses [bloom](https://wiki.ros.org/bloom) to generate Debian packaging from
`package.xml` and builds via `fakeroot debian/rules binary`. Output is written
to `dist/`.

```sh
make docker-deb                 # both packages, rolling
make docker-deb-humble          # both packages, specific distro
make docker-deb-bridge          # foxglove_bridge only, rolling
make docker-deb-bridge-humble   # foxglove_bridge only, specific distro
make docker-deb-msgs            # foxglove_msgs only, rolling
make docker-deb-msgs-humble     # foxglove_msgs only, specific distro
```

## Using a pre-built C++ SDK

By default, the ROS build fetches the C++ SDK sources via CMake's
`FetchContent`. For faster iteration you can point the build at a local
pre-built SDK instead.

First, build the SDK distribution from the repo root:

```sh
make build-cpp-dist    # outputs to cpp/dist/
```

Then pass the path (as seen inside the container) to the ROS build:

```sh
make docker-build FOXGLOVE_CPP_SDK_DIR=/sdk/cpp/dist
```

The volume mount maps the repo root to `/sdk` inside the container, so
`/sdk/cpp/dist` corresponds to `cpp/dist/` on the host.
