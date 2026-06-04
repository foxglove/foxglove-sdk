# foxglove_bridge_ros1

ROS 1 frontend for the Foxglove bridge, built on `foxglove_bridge_core`.
Connects ROS 1 topics, services, and parameters to Foxglove clients over a
local WebSocket server and, when enabled, the Foxglove remote access gateway
(LiveKit/WebRTC — handled entirely by the core/SDK).

## Building

Noetic is only released for Ubuntu 20.04 (focal, glibc 2.31), but the SDK's
remote access support requires glibc >= 2.35. The supported build is therefore
a from-source Noetic on Ubuntu 22.04 (jammy), via Docker. From the repo root:

```sh
make build-cpp-dist          # once: jammy-built SDK dist with remote access
cd ros && make docker-build-noetic
```

Run against an external rosmaster:

```sh
docker run --rm --network host \
  -e ROS_MASTER_URI=http://localhost:11311 \
  -e FOXGLOVE_DEVICE_TOKEN=<token> \
  foxglove-bridge-ros1-noetic \
  rosrun foxglove_bridge_ros1 foxglove_bridge _remote_access:=true
```

## Implementation notes

- **Schemas/md5sums** come from `ros_babel_fish`'s integrated description
  provider (disk lookup at advertise time), following the legacy
  `foxglove/ros-foxglove-bridge` design.
- **Topic/service/graph discovery** polls the master (`getTopicTypes`,
  `getSystemState`) with exponential backoff (100ms doubling to
  `~max_update_ms`, default 5000).
- **Subscriptions** use `topic_tools::ShapeShifter` and forward raw serialized
  bytes; **client publishers** are created from `ros::AdvertiseOptions` with
  babel_fish-provided type info, and inbound messages are republished via a
  morphed ShapeShifter.
- **Services** are called generically: the type is probed from the service
  server's connection header (`service_utils.cpp`, ported from the legacy
  bridge), the md5 looked up via babel_fish, and raw bytes forwarded with a
  dynamic-traits `GenericService`.
- **Parameters** implement the core's `ParameterBackend` over `ros::param`;
  subscriptions are change-detected by polling (`~param_poll_interval_ms`,
  default 1000).

## Known limitations / TODOs

- rosconsole in the Docker image is built with the `print` backend (jammy's
  log4cxx 0.12 is API-incompatible with noetic's rosconsole), so named-logger
  services (`get_loggers`) report an empty hierarchy. To be fixed.
- Parameter updates are poll-based; the master's `subscribeParam` push
  mechanism would need a dedicated XML-RPC endpoint (see legacy bridge).
- No latched-topic replay for late-joining clients (parity with the legacy
  bridge), and no asset fetching yet.
- `ROS_VERSION == 1` conditions plus a `COLCON_IGNORE` marker keep ROS 2
  colcon/rosdep away from this package; `Dockerfile.noetic` removes the marker
  in its private workspace copy because modern `catkin_pkg` honors
  COLCON_IGNORE as well.
