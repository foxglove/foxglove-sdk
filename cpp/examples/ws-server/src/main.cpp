#include <foxglove/channel.hpp>
#include <foxglove/server.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <memory>
#include <thread>

using namespace std::chrono_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigintHandler;

// NOLINTNEXTLINE(bugprone-exception-escape)
int main(int argc, const char* argv[]) {
  std::signal(SIGINT, [](int) {
    if (sigintHandler) {
      sigintHandler();
    }
  });

  foxglove::WebSocketServerOptions options = {};
  options.name = "ws-demo-cpp";
  options.host = "127.0.0.1";
  options.port = 8765;
  options.callbacks.onSubscribe = [](uint64_t channel_id) {
    std::cerr << "Subscribed to channel " << channel_id << '\n';
  };
  options.callbacks.onUnsubscribe = [](uint64_t channel_id) {
    std::cerr << "Unsubscribed from channel " << channel_id << '\n';
  };
  foxglove::WebSocketServer server{options};
  std::cerr << "Server listening on port " << server.port() << '\n';

  std::atomic_bool done = false;
  sigintHandler = [&] {
    std::cerr << "Shutting down...\n";
    server.stop();
    done = true;
  };

  foxglove::Schema schema;
  schema.name = "Test";
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

  uint32_t i = 0;
  while (!done) {
    std::this_thread::sleep_for(100ms);
    std::string msg = "{\"val\": " + std::to_string(i) + "}";
    auto now =
      std::chrono::nanoseconds(std::chrono::system_clock::now().time_since_epoch()).count();
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size(), now, now, i);
    ++i;
  }

  std::cerr << "Done\n";
  return 0;
}
