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

static void MessageReceivedHandler(uint32_t subscription_id, std::atomic<uint64_t>& message_count, const uint8_t* data, size_t dataLength) {
  if (dataLength < 1 + 4 + 8 || foxglove::test::ReadUint32LE(data + 1) != subscription_id) {
    return;
  }
  message_count.fetch_add(1, std::memory_order_relaxed);
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
    _executor.reset();
    rclcpp::shutdown();
  }

  void addNode(const std::string& nodeName, std::unique_ptr<rclcpp::Node>&& node) {
    _publisherNodes[nodeName] = std::move(node);
    _executor->add_node(_publisherNodes[nodeName]->get_node_base_interface());
  }

  // Create a client, connect to the bridge, and subscribe to a topic. Subscription ID is automatically generated.
  // If the client fails to connect or subscribe, returns a pair with a null pointer and 0.
  std::pair<std::unique_ptr<BenchmarkClient>, uint32_t> createClient(const std::string& topicName) {
    auto client = std::make_unique<BenchmarkClient>();
    auto channel_future = client->waitForChannel(topicName);
    auto connection_status = client->connect("ws://localhost:" + std::to_string(port)).wait_for(std::chrono::seconds(1));
    if (connection_status == std::future_status::timeout) {
      return {nullptr, 0};
    }

    if (channel_future.wait_for(std::chrono::seconds(10)) == std::future_status::timeout) {
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

  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  // Set up a client
  std::unique_ptr<BenchmarkClient> client;
  uint32_t subscription_id;
  std::tie(client, subscription_id) = createClient(topic_name);
  if (!client) {
    state.SkipWithError("Client failed to set up");
    return;
  }

  std::atomic<uint64_t> message_count{0};
  client->setBinaryMessageHandler(std::bind(MessageReceivedHandler, subscription_id, std::ref(message_count), _1, _2));

  // Hack to avoid race condition with the message handler being initialized after we enter the benchmark loop
  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  std_msgs::msg::String msg;
  msg.data = "Hello, world!";

  for (auto _ : state) {
    const uint64_t start_count = message_count.load(std::memory_order_relaxed);
    publisher_publisher->publish(msg);

    while (message_count.load(std::memory_order_relaxed) == start_count) {
      std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }
  }

  // Unsubscribe after receiving the message
  client->unsubscribe({subscription_id});
  client->close();
}

BENCHMARK_F(BridgeBenchmarkFixture, BM_RandomImagePublish)(benchmark::State& state) {
  constexpr char topic_name[] = "/image_test";

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

  std::atomic<uint64_t> message_count{0};
  client->setBinaryMessageHandler(std::bind(MessageReceivedHandler, subscription_id, std::ref(message_count), _1, _2));

  std::this_thread::sleep_for(std::chrono::milliseconds(500));

  for (auto _ : state) {
    const uint64_t start_count = message_count.load(std::memory_order_relaxed);
    publisher_publisher->publish(image_msg);

    while (message_count.load(std::memory_order_relaxed) == start_count) {
      std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }
  }
}

// Multi-client tracking now uses MessageReceivedHandler with per-client flags

BENCHMARK_F(BridgeBenchmarkFixture, BM_RandomImageMultipleClients)(benchmark::State& state) {
  constexpr char topic_name[] = "/image_queue_multi";
  constexpr size_t num_clients = 10;

  // Set up a publisher node
  auto publisher = std::make_unique<rclcpp::Node>("log_publisher");
  auto publisher_publisher =
    publisher->create_publisher<foxglove_msgs::msg::RawImage>(topic_name, rclcpp::QoS(rclcpp::KeepLast(10)));
  addNode("log_publisher", std::move(publisher));

  // TODO: This is a hack to wait for the bridge to be ready.
  std::this_thread::sleep_for(std::chrono::milliseconds(100));

  // Create WebSocket clients
  std::vector<std::unique_ptr<BenchmarkClient>> clients;
  std::vector<uint32_t> subscription_ids;
  std::atomic<uint64_t> client_counts{0};

  for (size_t i = 0; i < num_clients; ++i) {
    std::unique_ptr<BenchmarkClient> client;
    uint32_t subscription_id;
    std::tie(client, subscription_id) = createClient(topic_name);
    if (!client) {
      state.SkipWithError("Client " + std::to_string(i) + " failed to set up");
      return;
    }

    // Initialize counter and set up message handler for this client
    client->setBinaryMessageHandler(std::bind(MessageReceivedHandler, subscription_id, std::ref(client_counts), _1, _2));

    clients.push_back(std::move(client));
    subscription_ids.push_back(subscription_id);
  }

  // Hack to avoid race condition with the message handlers being initialized
  std::this_thread::sleep_for(std::chrono::milliseconds(200));

  // Generate a random image
  constexpr size_t width = 1920;
  constexpr size_t height = 1080;
  foxglove_msgs::msg::RawImage image;
  image.width = width;
  image.height = height;
  image.encoding = "rgb8";
  image.data.resize(width * height * 3);
  std::generate(image.data.begin(), image.data.end(), []() { return rand() % 256; });

  for (auto _ : state) {
    // Reset per-iteration aggregated counter
    client_counts.store(0, std::memory_order_relaxed);

    publisher_publisher->publish(image);

    // Wait until all clients have received the image
    while (client_counts.load(std::memory_order_relaxed) < num_clients) {
      std::this_thread::sleep_for(std::chrono::nanoseconds(100));
    }
  }

  // Clean up: unsubscribe and close all clients
  for (size_t i = 0; i < num_clients; ++i) {
    clients[i]->unsubscribe({subscription_ids[i]});
    clients[i]->close();
  }
}

BENCHMARK_MAIN();
