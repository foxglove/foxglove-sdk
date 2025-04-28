#include <foxglove/channel.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/server.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <thread>

using namespace std::chrono_literals;

// This example logs custom data on an "example" topic. Open Foxglove and connect to the running
// server. Then add a Raw Message panel, and choose the "example" topic.
int main(int argc, const char* argv[]) {
  static std::function<void()> sigint_handler;

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  // We'll log to both an MCAP file, and to a running Foxglove app.
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

  std::atomic_bool done = false;
  sigint_handler = [&] {
    done = true;
  };

  // Our example logs custom data on an "example" topic, so we'll create a channel for that.
  foxglove::Schema schema;
  schema.encoding = "jsonschema";
  std::string schema_data = R"({
    "type": "object",
    "properties": {
      "val": { "type": "number" }
    }
  })";
  schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  schema.data_len = schema_data.size();
  auto channel_result = foxglove::Channel::create("example", "json", std::move(schema));
  if (!channel_result.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channel_result.value());

  while (!done) {
    // Log messages on the channel until interrupted. By default, each message
    // is stamped with the current time.
    std::this_thread::sleep_for(33ms);
    auto now = std::chrono::system_clock::now().time_since_epoch().count();
    std::string msg = "{\"val\": " + std::to_string(now) + "}";
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());
  }

  return 0;
}
