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
  options.capabilities = foxglove::WebSocketServerCapabilities::ClientPublish;
  options.supportedEncodings = {"json"};
  options.callbacks.onSubscribe = [](uint64_t channel_id) {
    std::cerr << "Subscribed to channel " << channel_id << '\n';
  };
  options.callbacks.onUnsubscribe = [](uint64_t channel_id) {
    std::cerr << "Unsubscribed from channel " << channel_id << '\n';
  };
  options.callbacks.onClientAdvertise = [](
                                          uint32_t clientId, const foxglove::ClientChannel& channel
                                        ) {
    std::cerr << "Client " << clientId << " advertised channel " << channel.id << ":\n";
    std::cerr << "  Topic: " << channel.topic << '\n';
    std::cerr << "  Encoding: " << channel.encoding << '\n';
    std::cerr << "  Schema name: " << channel.schemaName << '\n';
    std::cerr << "  Schema encoding: "
              << (!channel.schemaEncoding.empty() ? channel.schemaEncoding : "(none)") << '\n';
    std::cerr << "  Schema: "
              << (channel.schema != nullptr
                    ? std::string(reinterpret_cast<const char*>(channel.schema), channel.schemaLen)
                    : "(none)")
              << '\n';
  };
  options.callbacks.onMessageData =
    [](uint32_t clientId, uint32_t clientChannelId, const std::byte* data, size_t dataLen) {
      std::cerr << "Client " << clientId << " published on channel " << clientChannelId << ": "
                << std::string(reinterpret_cast<const char*>(data), dataLen) << '\n';
    };
  options.callbacks.onClientUnadvertise = [](uint32_t clientId, uint32_t clientChannelId) {
    std::cerr << "Client " << clientId << " unadvertised channel " << clientChannelId << '\n';
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
