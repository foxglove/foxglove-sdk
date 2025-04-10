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

static std::function<void()> sigintHandler;

int main(int argc, const char* argv[]) {
  std::signal(SIGINT, [](int) {
    if (sigintHandler) {
      sigintHandler();
    }
  });

  foxglove::WebSocketServerOptions ws_options;
  ws_options.host = "127.0.0.1";
  ws_options.port = 8765;
  foxglove::WebSocketServer server{ws_options};
  std::cerr << "Server listening on port " << server.port() << '\n';

  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.path = "quickstart-cpp.mcap";
  foxglove::McapWriter writer(mcap_options);

  std::atomic_bool done = false;
  sigintHandler = [&] {
    done = true;
  };

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
  foxglove::RawChannel jsonChannel{"example_json", "json", std::move(schema)};
  foxglove::Channel<foxglove::schemas::Vector3> vec3Channel{"example_vec"};

  while (!done) {
    std::this_thread::sleep_for(33ms);
    auto now = std::chrono::system_clock::now().time_since_epoch().count();
    std::string msg = "{\"val\": " + std::to_string(now) + "}";
    jsonChannel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());
    foxglove::schemas::Vector3 vec = {double(now), double(now + 1), double(now + 2)};
    vec3Channel.log(vec);
  }

  return 0;
}
