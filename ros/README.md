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
