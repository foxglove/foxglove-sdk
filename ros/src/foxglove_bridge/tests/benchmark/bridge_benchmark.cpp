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

  // Store port in a ROS parameter that bridge is reading
  rclcpp::NodeOptions options;
  options.parameter_overrides({{"port", port}});

  // Set up a bridge
  foxglove_bridge::FoxgloveBridge bridge(options);

  // Set up a publisher node
  auto publisher = std::make_shared<rclcpp::Node>("publisher", options);
  auto publisher_publisher =
    publisher->create_publisher<std_msgs::msg::String>(topic_name, rclcpp::QoS(rclcpp::KeepLast(10)));

  // Setup executor for manual control in benchmark loop
  rclcpp::executors::SingleThreadedExecutor executor;
  executor.add_node(bridge.get_node_base_interface());
  executor.add_node(publisher->get_node_base_interface());

  std::thread executor_thread([&]() {
    executor.spin();
  });

  // TODO: This is a hack to wait for the bridge to be ready.
  // There's a race condition where the rust side can advertise the channel
  // before the C++ side fully initializes it and inserts it into _channels.
  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  // Create WebSocket clients with reliable connection handling
  auto client = std::make_unique<BenchmarkClient>();
  constexpr size_t subscription_id = 100;

  auto connection_future = client->connect("ws://localhost:" + std::to_string(port));
  if (connection_future.wait_for(std::chrono::seconds(1)) == std::future_status::timeout) {
    state.SkipWithError("Client failed to connect after 1 second");
    return;
  }

  auto channel_future = client->waitForChannel(topic_name);
  if (channel_future.wait_for(std::chrono::seconds(1)) == std::future_status::timeout) {
    state.SkipWithError("Channel " + std::string(topic_name) + " not found after 1 second");
    return;
  }

  auto channel = channel_future.get();
  client->subscribe({{subscription_id, channel.id}});

  std_msgs::msg::String msg;
  msg.data = "Hello, world!";

  std::atomic<bool> message_received = false;
  auto message_handler = [&message_received](const uint8_t* data, size_t dataLength) {
    if (dataLength < 1 + 4 + 8 || foxglove::test::ReadUint32LE(data + 1) != subscription_id) {
      return;
    }
    message_received = true;
  };
  client->setBinaryMessageHandler(message_handler);

  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  for (auto _ : state) {
    message_received = false;
    publisher_publisher->publish(msg);

    while (!message_received) {
      std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }
  }

  // Unsubscribe after receiving the message
  client->unsubscribe({subscription_id});

  executor.cancel();
  executor_thread.join();
  executor.remove_node(bridge.get_node_base_interface());
  executor.remove_node(publisher->get_node_base_interface());
  client->close();
}

BENCHMARK(BM_StringPublish)->Setup(DoSetup)->Teardown(DoTeardown);

BENCHMARK_MAIN();
