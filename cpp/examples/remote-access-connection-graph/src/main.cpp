#include <foxglove/foxglove.hpp>
#include <foxglove/remote_access.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <thread>

using namespace std::chrono_literals;

/**
 * This example constructs a connection graph which can be viewed as a Topic Graph in Foxglove:
 * https://docs.foxglove.dev/docs/visualization/panels/topic-graph
 *
 * Unlike the WebSocket connection-graph example, this uses the remote access gateway.
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

  foxglove::setLogLevel(foxglove::LogLevel::Info);

  foxglove::RemoteAccessGatewayOptions options = {};
  options.name = "remote-access-connection-graph-cpp";
  options.capabilities = foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  options.callbacks.onConnectionStatusChanged = [](foxglove::RemoteAccessConnectionStatus status) {
    const char* label = "unknown";
    switch (status) {
      case foxglove::RemoteAccessConnectionStatus::Connecting:
        label = "connecting";
        break;
      case foxglove::RemoteAccessConnectionStatus::Connected:
        label = "connected";
        break;
      case foxglove::RemoteAccessConnectionStatus::ShuttingDown:
        label = "shutting down";
        break;
      case foxglove::RemoteAccessConnectionStatus::Shutdown:
        label = "shutdown";
        break;
    }
    std::cerr << "Connection status: " << label << '\n';
  };
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

  auto gateway_result = foxglove::RemoteAccessGateway::create(std::move(options));
  if (!gateway_result.has_value()) {
    std::cerr << "Failed to create gateway: " << foxglove::strerror(gateway_result.error()) << '\n';
    std::cerr << "Set the FOXGLOVE_DEVICE_TOKEN environment variable.\n";
    return 1;
  }
  auto gateway = std::move(gateway_result.value());

  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    done = true;
  };

  while (!done) {
    auto err2 = gateway.publishConnectionGraph(graph);
    if (err2 != foxglove::FoxgloveError::Ok) {
      std::cerr << "Failed to publish connection graph: " << foxglove::strerror(err2) << '\n';
    }
    std::this_thread::sleep_for(1s);
  }

  gateway.stop();
  std::cerr << "Done\n";
  return 0;
}
