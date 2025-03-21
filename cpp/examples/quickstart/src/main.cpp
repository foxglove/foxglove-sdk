#include <foxglove/channel.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/server.hpp>

#include <csignal>
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
  foxglove::Channel channel{"example", "json", std::move(schema)};

  while (!done) {
    std::this_thread::sleep_for(33ms);
    auto now =
      std::chrono::system_clock::now().time_since_epoch().count();
    std::string msg = "{\"val\": " + std::to_string(now) + "}";
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());
  }

  return 0;
}
