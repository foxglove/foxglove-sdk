#include <foxglove_bridge/ros2_foxglove_bridge.hpp>

#include <benchmark/benchmark.h>
#include <rclcpp/rclcpp.hpp>
#include <std_msgs/msg/string.hpp>
#include <websocketpp/config/asio_client.hpp>

#include <chrono>
#include <string>
#include <thread>

#include "../client/test_client.hpp"
#include "foxglove/foxglove.hpp"

// Some interesting benchmarks:
// - Send a bunch of small messages through the bridge
// - Send a bunch of large messages (like images) through the bridge
// - Send a bunch of small messages through the bridge from a parallel series of publishers
// - Send a bunch of messages through the bridge with a bunch of subscribers connected

using BenchmarkClient = foxglove::test::Client<websocketpp::config::asio_client>;

static void DoSetup(const benchmark::State& state [[maybe_unused]]) {
  rclcpp::init(0, nullptr);
}

static void DoTeardown(const benchmark::State& state [[maybe_unused]]) {
  rclcpp::shutdown();
}

static void BM_StringPublish(benchmark::State& state) {
  constexpr char topic_name[] = "/test";
  constexpr uint16_t port = 8765;
  constexpr size_t num_clients = 10;

  // Store port in a ROS parameter that bridge is reading
  rclcpp::NodeOptions options;
  options.parameter_overrides({{"port", port}});

  // Set up a bridge
  foxglove_bridge::FoxgloveBridge bridge(options);

  // Set up a publisher
  auto publisher =
    bridge.create_publisher<std_msgs::msg::String>(topic_name, rclcpp::QoS(rclcpp::KeepLast(10)));
  std_msgs::msg::String msg;
  msg.data = "Hello, world!";

  // Setup executor for manual control in benchmark loop
  rclcpp::executors::SingleThreadedExecutor executor;
  executor.add_node(bridge.get_node_base_interface());

  // Minimal bridge initialization - let client creation handle readiness validation
  executor.spin_some();

  // Create WebSocket clients with reliable connection handling
  std::vector<std::unique_ptr<BenchmarkClient>> clients;
  clients.reserve(num_clients);

  auto createClientReliably = [&](size_t clientId) -> std::unique_ptr<BenchmarkClient> {
    // Retry logic with executor spinning for connection reliability
    for (int attempt = 0; attempt < 3; ++attempt) {
      auto client = std::make_unique<foxglove::test::Client<websocketpp::config::asio_client>>();

      std::string uri = "ws://localhost:" + std::to_string(port);
      auto connect_future = client->connect(uri);

      // Wait with executor spinning for bridge responsiveness
      auto connection_start = std::chrono::steady_clock::now();

      while (std::chrono::steady_clock::now() - connection_start < std::chrono::seconds(3)) {
        if (connect_future.wait_for(std::chrono::milliseconds(100)) == std::future_status::ready) {
          break;
        }
        executor.spin_some();
      }

      if (connect_future.wait_for(std::chrono::milliseconds(0)) != std::future_status::ready) {
        if (attempt < 2) {
          // Brief delay with executor spinning before retry
          auto delay_start = std::chrono::steady_clock::now();
          auto delay_duration = std::chrono::milliseconds(100 * (attempt + 1));
          while (std::chrono::steady_clock::now() - delay_start < delay_duration) {
            executor.spin_some();
            std::this_thread::yield();
          }
        }
        continue;
      }

      // Wait for subscription with executor spinning
      auto subscribe_future = client->waitForChannel(topic_name);
      auto subscribe_start = std::chrono::steady_clock::now();

      while (std::chrono::steady_clock::now() - subscribe_start < std::chrono::seconds(2)) {
        if (subscribe_future.wait_for(std::chrono::milliseconds(100)) ==
            std::future_status::ready) {
          break;
        }
        executor.spin_some();
      }

      if (subscribe_future.wait_for(std::chrono::milliseconds(0)) == std::future_status::ready) {
        return client;  // Success!
      }

      // Subscription failed, brief delay before retry
      if (attempt < 2) {
        auto delay_start = std::chrono::steady_clock::now();
        auto delay_duration = std::chrono::milliseconds(50 * (attempt + 1));
        while (std::chrono::steady_clock::now() - delay_start < delay_duration) {
          executor.spin_some();
          std::this_thread::yield();
        }
      }
    }

    // All attempts failed
    std::cerr << "Client " << clientId << " failed to connect after 3 attempts" << std::endl;
    return nullptr;
  };

  // Create clients with executor spinning for reliability
  for (size_t i = 0; i < num_clients; ++i) {
    auto client = createClientReliably(i);
    if (client) {
      clients.push_back(std::move(client));
    }
  }

  if (clients.size() < num_clients) {
    state.SkipWithError(
      "Failed to connect all clients reliably. Connected: " + std::to_string(clients.size()) + "/" +
      std::to_string(num_clients)
    );
    return;
  }

  // Benchmark: Total server throughput (includes processing and WebSocket transmission)
  for (auto _ : state) {
    publisher->publish(msg);
    executor.spin_some();  // Process and deliver messages to all 10 WebSocket clients
  }
}

BENCHMARK(BM_StringPublish)->Setup(DoSetup)->Teardown(DoTeardown);

BENCHMARK_MAIN();
