#include <foxglove/channel.hpp>
#include <foxglove/foxglove.hpp>
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
static std::function<void()> sigint_handler;

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::setLogLevel(foxglove::LogLevel::Debug);

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

  auto graph = foxglove::ConnectionGraph();
  auto err = graph.setPublishedTopic("/example-topic", {"1", "2"});
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to set published topic: " << foxglove::strerror(err) << '\n';
  }
  graph.setSubscribedTopic("/subscribed-topic", {"3", "4"});
  graph.setAdvertisedService("example-service", {"5", "6"});

  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error()) << '\n';
    return 1;
  }
  auto server = std::move(server_result.value());

  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    server.stop();
    done = true;
  };

  while (!done) {
    server.publishConnectionGraph(graph);
    std::this_thread::sleep_for(1s);
  }

  std::cerr << "Done\n";
  return 0;
}
