#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/schemas.hpp>

#include <chrono>
#include <cmath>
#include <iostream>

foxglove::schemas::SceneUpdateChannel SCENE_UPDATE_CHANNEL =
  foxglove::schemas::SceneUpdateChannel::create("/boxes").value();
foxglove::schemas::FrameTransformChannel FRAME_TRANSFORM_CHANNEL =
  foxglove::schemas::FrameTransformChannel::create("/tf").value();

void log_to_channels(int counter) {
  // Create a SceneUpdate message for the box
  foxglove::schemas::SceneUpdate scene_update;
  foxglove::schemas::SceneEntity entity;
  entity.frame_id = "box";
  entity.id = "box_1";
  entity.lifetime = foxglove::schemas::Duration{10, 10000000};

  // Create a cube primitive
  foxglove::schemas::CubePrimitive cube;
  foxglove::schemas::Pose pose;
  foxglove::schemas::Vector3 position;
  position.x = 0.0;
  position.y = 0.0;
  position.z = 3.0;
  pose.position = position;

  foxglove::schemas::Quaternion orientation;
  double yaw = -0.1 * counter;
  orientation.x = 0.0;
  orientation.y = 0.0;
  orientation.z = std::sin(yaw / 2.0);
  orientation.w = std::cos(yaw / 2.0);
  pose.orientation = orientation;
  cube.pose = pose;

  // Set cube size
  foxglove::schemas::Vector3 size;
  size.x = 1.0;
  size.y = 1.0;
  size.z = 1.0;
  cube.size = size;

  // Set cube color (red)
  foxglove::schemas::Color color;
  color.r = 1.0;
  color.g = 0.0;
  color.b = 0.0;
  color.a = 1.0;
  cube.color = color;

  entity.cubes.push_back(cube);
  scene_update.entities.push_back(entity);
  SCENE_UPDATE_CHANNEL.log(scene_update);

  // Create a FrameTransform message
  foxglove::schemas::FrameTransform transform;
  transform.parent_frame_id = "world";
  transform.child_frame_id = "box";

  foxglove::schemas::Quaternion rotation;
  double yaw2 = 0.1 * counter;
  double roll = 1.0;
  rotation.x = std::sin(roll / 2.0);
  rotation.y = 0.0;
  rotation.z = std::sin(yaw2 / 2.0);
  rotation.w = std::cos(yaw2 / 2.0);
  transform.rotation = rotation;

  FRAME_TRANSFORM_CHANNEL.log(transform);
}

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  auto writer_result = foxglove::McapWriter::create(options);
  if (!writer_result.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_result.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writer_result.value());

  for (int i = 0; i < 100; ++i) {
    log_to_channels(i);
  }

  // Optional, if you want to check for or handle errors
  foxglove::FoxgloveError err = writer.close();
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to close writer: " << foxglove::strerror(err) << '\n';
    return 1;
  }
  return 0;
}
