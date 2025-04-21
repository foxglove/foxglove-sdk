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
  static std::function<void()> sigintHandler;

  std::signal(SIGINT, [](int) {
    if (sigintHandler) {
      sigintHandler();
    }
  });

  // We'll log to both an MCAP file, and to a running Foxglove app.
  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.path = "quickstart-cpp.mcap";
  auto writerResult = foxglove::McapWriter::create(mcap_options);
  if (!writerResult.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writerResult.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writerResult.value());

  // Start a server to communicate with the Foxglove app.
  foxglove::WebSocketServerOptions ws_options;
  ws_options.host = "127.0.0.1";
  ws_options.port = 8765;
  auto serverResult = foxglove::WebSocketServer::create(std::move(ws_options));
  if (!serverResult.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(serverResult.error()) << '\n';
    return 1;
  }
  auto server = std::move(serverResult.value());
  std::cerr << "Server listening on port " << server.port() << '\n';

  std::atomic_bool done = false;
  sigintHandler = [&] {
    done = true;
  };

  // Our example logs custom data on an "example" topic, so we'll create a channel for that.
  foxglove::Schema schema;
  schema.encoding = "jsonschema";
  std::string schemaData = R"({
    "type": "object",
    "properties": {
      "val": { "type": "number" }
    }
  })";
  schema.data = reinterpret_cast<const std::byte*>(schemaData.data());
  schema.dataLen = schemaData.size();
  auto channelResult = foxglove::Channel::create("example", "json", std::move(schema));
  if (!channelResult.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channelResult.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channelResult.value());

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
