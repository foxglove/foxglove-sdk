#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/messages.hpp>
#include <foxglove/websocket.hpp>

#include <atomic>
#include <chrono>
#include <cmath>
#include <csignal>
#include <functional>
#include <iostream>
#include <thread>

using namespace std::chrono_literals;

int main() {
  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  static std::function<void()> sigint_handler;

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.path = "quickstart-cpp.mcap";
  auto writer_result = foxglove::McapWriter::create(mcap_options);
  if (!writer_result.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_result.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writer_result.value());

  // Start a server to communicate with the Foxglove app.
  foxglove::WebSocketServerOptions ws_options;
  ws_options.host = "127.0.0.1";
  ws_options.port = 8765;
  auto server_result = foxglove::WebSocketServer::create(std::move(ws_options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error()) << '\n';
    return 1;
  }
  auto server = std::move(server_result.value());
  std::cerr << "Server listening on port " << server.port() << '\n';

  // Create a schema for a JSON channel for logging {size: number}
  foxglove::Schema schema;
  schema.encoding = "jsonschema";
  std::string schema_data = R"({
        "type": "object",
        "properties": {
        "size": { "type": "number" }
        }
    })";
  schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  schema.data_len = schema_data.size();
  auto channel_result = foxglove::RawChannel::create("/size", "json", std::move(schema));
  if (!channel_result.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto size_channel = std::move(channel_result.value());

  // Create a SceneUpdateChannel for logging changes to a 3d scene
  auto scene_channel_result = foxglove::messages::SceneUpdateChannel::create("/scene");
  if (!scene_channel_result.has_value()) {
    std::cerr << "Failed to create scene channel: "
              << foxglove::strerror(scene_channel_result.error()) << '\n';
    return 1;
  }
  auto scene_channel = std::move(scene_channel_result.value());

  std::atomic_bool done = false;
  sigint_handler = [&] {
    done = true;
  };

  while (!done) {
    auto now = std::chrono::duration_cast<std::chrono::duration<double>>(
                 std::chrono::system_clock::now().time_since_epoch()
    )
                 .count();
    double size = std::abs(std::sin(now)) + 1.0;
    std::string msg = "{\"size\": " + std::to_string(size) + "}";
    size_channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());

    foxglove::messages::CubePrimitive cube;
    cube.size = foxglove::messages::Vector3{size, size, size};
    cube.color = foxglove::messages::Color{1, 0, 0, 1};

    foxglove::messages::SceneEntity entity;
    entity.id = "box";
    entity.cubes.push_back(cube);

    foxglove::messages::SceneUpdate scene_update;
    scene_update.entities.push_back(entity);

    scene_channel.log(scene_update);

    std::this_thread::sleep_for(33ms);
  }

  return 0;
}
