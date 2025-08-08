#include <foxglove_bridge/ros2_foxglove_bridge.hpp>

#include <benchmark/benchmark.h>
#include <rclcpp/rclcpp.hpp>
#include <std_msgs/msg/string.hpp>
#include <foxglove_msgs/msg/raw_image.hpp>
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
using namespace std::placeholders;

static void MessageReceivedHandler(uint32_t subscription_id, std::atomic<bool>& message_received, const uint8_t* data, size_t dataLength) {
  if (dataLength < 1 + 4 + 8 || foxglove::test::ReadUint32LE(data + 1) != subscription_id) {
    return;
  }
  message_received = true;
}


class BridgeBenchmarkFixture : public ::benchmark::Fixture {
  public:
  constexpr static uint16_t port = 8765;

  void SetUp(::benchmark::State& state [[maybe_unused]]) {
    rclcpp::init(0, nullptr);
    rclcpp::NodeOptions options;
    options.parameter_overrides({{"port", port}});
    _bridge = std::make_unique<foxglove_bridge::FoxgloveBridge>(options);
    _executor = std::make_unique<rclcpp::executors::SingleThreadedExecutor>();
    _executor->add_node(_bridge->get_node_base_interface());

    _executorThread = std::thread([this]() {
      _executor->spin();
    });
  }

  void TearDown(::benchmark::State& state [[maybe_unused]]) {
    _executor->cancel();
    _executorThread.join();
    for (auto& [_, node] : _publisherNodes) {
      _executor->remove_node(node->get_node_base_interface());
    }
    _publisherNodes.clear();
    _executor->remove_node(_bridge->get_node_base_interface());
    _bridge.reset();
    rclcpp::shutdown();
  }

  void addNode(const std::string& nodeName, std::unique_ptr<rclcpp::Node>&& node) {
    _publisherNodes[nodeName] = std::move(node);
    _executor->add_node(_publisherNodes[nodeName]->get_node_base_interface());
  }

  // Create a client, connect to the bridge, and subscribe to a topic. Subscription ID is automatically generated.
  // If the client fails to connect or subscribe, returns a future with a null pointer.
  std::pair<std::unique_ptr<BenchmarkClient>, uint32_t> createClient(const std::string& topicName) {
    std::lock_guard<std::mutex> lock(_clientMutex);
    auto client = std::make_unique<BenchmarkClient>();
    auto connection_future = client->connect("ws://localhost:" + std::to_string(port));
    if (connection_future.wait_for(std::chrono::seconds(1)) == std::future_status::timeout) {
      return {nullptr, 0};
    }

    auto channel_future = client->waitForChannel(topicName);
    if (channel_future.wait_for(std::chrono::seconds(1)) == std::future_status::timeout) {
      return {nullptr, 0};
    }

    auto channel = channel_future.get();
    client->subscribe({{_subscriptionId, channel.id}});
    return {std::move(client), _subscriptionId++};
  }

  rclcpp::Node& node(const std::string& nodeName) {
    return *_publisherNodes[nodeName];
  }

private:
  std::mutex _clientMutex;
  uint32_t _subscriptionId = 1;
  std::unique_ptr<rclcpp::executors::SingleThreadedExecutor> _executor;
  std::thread _executorThread;
  std::unique_ptr<foxglove_bridge::FoxgloveBridge> _bridge;
  std::unordered_map<std::string, std::unique_ptr<rclcpp::Node>> _publisherNodes;
};

BENCHMARK_F(BridgeBenchmarkFixture, BM_StringPublish)(benchmark::State& state) {
  constexpr char topic_name[] = "/test";

  // Set up a publisher node
  auto publisher = std::make_unique<rclcpp::Node>("publisher");
  auto publisher_publisher =
    publisher->create_publisher<std_msgs::msg::String>(topic_name, rclcpp::QoS(rclcpp::KeepLast(10)));
  addNode("publisher", std::move(publisher));

  // TODO: This is a hack to wait for the bridge to be ready.
  // There's a race condition where the rust side can advertise the channel
  // before the C++ side fully initializes it and inserts it into _channels.
  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  // Create WebSocket clients with reliable connection handling
  std::unique_ptr<BenchmarkClient> client;
  uint32_t subscription_id;
  std::tie(client, subscription_id) = createClient(topic_name);
  if (!client) {
    state.SkipWithError("Client failed to set up");
    return;
  }

  std::atomic<bool> message_received = false;
  client->setBinaryMessageHandler(std::bind(MessageReceivedHandler, subscription_id, std::ref(message_received), _1, _2));

  // Hack to avoid race condition with the message handler being initialized after we enter the benchmark loop
  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  std_msgs::msg::String msg;
  msg.data = "Hello, world!";
  for (auto _ : state) {
    message_received = false;
    publisher_publisher->publish(msg);

    while (!message_received) {
      std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }
  }

  // Unsubscribe after receiving the message
  client->unsubscribe({subscription_id});
  client->close();
}

BENCHMARK_F(BridgeBenchmarkFixture, BM_RandomImagePublish)(benchmark::State& state) {
  constexpr char topic_name[] = "/test";

  // Set up a publisher node
  auto publisher = std::make_unique<rclcpp::Node>("publisher");
  auto publisher_publisher =
    publisher->create_publisher<foxglove_msgs::msg::RawImage>(topic_name, rclcpp::QoS(rclcpp::KeepLast(10)));
  addNode("publisher", std::move(publisher));

  // TODO: This is a hack to wait for the bridge to be ready.
  // There's a race condition where the rust side can advertise the channel
  // before the C++ side fully initializes it and inserts it into _channels.
  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  // Generate a random image
  constexpr size_t width = 1920;
  constexpr size_t height = 1080;

  std::vector<uint8_t> image(width * height * 3);
  std::generate(image.begin(), image.end(), []() { return rand() % 256; });

  foxglove_msgs::msg::RawImage image_msg;
  image_msg.width = width;
  image_msg.height = height;
  image_msg.encoding = "rgb8";
  image_msg.data = image;

  std::unique_ptr<BenchmarkClient> client;
  uint32_t subscription_id;
  std::tie(client, subscription_id) = createClient(topic_name);
  if (!client) {
    state.SkipWithError("Client failed to set up");
    return;
  }

  std::atomic<bool> message_received = false;
  client->setBinaryMessageHandler(std::bind(MessageReceivedHandler, subscription_id, std::ref(message_received), _1, _2));

  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  // TODO: We still occasionally have issues where the websocket server is advertising the channel and the ROS side
  // complains with "received subscribe request for unknown channel". This is a race condition that needs to be fixed.
  for (auto _ : state) {
    message_received = false;
    publisher_publisher->publish(image_msg);

    while (!message_received) {
      std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }
  }
}

BENCHMARK_MAIN();
