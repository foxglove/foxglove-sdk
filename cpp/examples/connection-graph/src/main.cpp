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

/**
 * This example constructs a connection graph which can be viewed as a Topic Graph in Foxglove:
 * https://docs.foxglove.dev/docs/visualization/panels/topic-graph
 */

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
  options.capabilities = foxglove::WebSocketServerCapabilities::ConnectionGraph;
  options.callbacks.onConnectionGraphSubscribe = []() {
    std::cerr << "Connection graph subscribed\n";
  };
  options.callbacks.onConnectionGraphUnsubscribe = []() {
    std::cerr << "Connection graph unsubscribed\n";
  };

  auto connectionGraph = foxglove::ConnectionGraph();
  auto err = connectionGraph.setPublishedTopic("/example-topic", {"1", "2"});
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to set published topic: " << foxglove::strerror(err) << '\n';
  }
  connectionGraph.setSubscribedTopic("/subscribed-topic", {"3", "4"});
  connectionGraph.setAdvertisedService("example-service", {"5", "6"});

  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  if (!serverResult.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(serverResult.error()) << '\n';
    return 1;
  }
  auto server = std::move(serverResult.value());
  std::cerr << "Server listening on port " << server.port() << '\n';

  std::atomic_bool done = false;
  sigintHandler = [&] {
    std::cerr << "Shutting down...\n";
    server.stop();
    done = true;
  };

  uint32_t i = 0;
  while (!done) {
    std::this_thread::sleep_for(1s);
    server.publishConnectionGraph(connectionGraph);

    ++i;
  }

  std::cerr << "Done\n";
  return 0;
}
