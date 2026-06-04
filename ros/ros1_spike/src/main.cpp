// Spike: prove that the Foxglove SDK's RemoteAccessGateway builds, links, and
// connects from inside a Ubuntu 22.04 (jammy) container — the base we intend to
// use for a ROS 1 Noetic foxglove_bridge image (Noetic's official focal base has
// glibc 2.31 < 2.35 required by the SDK's remote access support).
//
// Always publishes a JSON timestamp on /spike/timestamp via a RawChannel,
// mirroring how the ROS 2 foxglove_bridge logs serialized ROS messages.
//
// When built with SPIKE_WITH_ROS (see Dockerfile.noetic), additionally acts as a
// minimal ROS 1 bridge: subscribes to /chatter with topic_tools::ShapeShifter
// and forwards the raw serialized bytes to a "ros1" RawChannel whose schema is
// taken from the connection header's message definition.

#include <foxglove/channel.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/remote_access.hpp>

#ifdef SPIKE_WITH_ROS
#include <ros/ros.h>
#include <topic_tools/shape_shifter.h>
#endif

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <memory>
#include <optional>
#include <string>
#include <thread>
#include <vector>

using namespace std::chrono_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

// NOLINTNEXTLINE(bugprone-exception-escape)
int main(int argc, char** argv) {
  (void)argc;
  (void)argv;

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });
  std::signal(SIGTERM, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  foxglove::RemoteAccessGatewayOptions options = {};
  options.name = "ros1-spike";
  options.supported_encodings = {"json", "ros1"};
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
    std::cerr << "[spike] connection status: " << label << '\n';
  };
  options.callbacks.onSubscribe = [](uint64_t channel_id, const foxglove::ChannelDescriptor&) {
    std::cerr << "[spike] client subscribed to channel " << channel_id << '\n';
  };
  options.callbacks.onUnsubscribe = [](uint64_t channel_id, const foxglove::ChannelDescriptor&) {
    std::cerr << "[spike] client unsubscribed from channel " << channel_id << '\n';
  };

  auto gateway_result = foxglove::RemoteAccessGateway::create(std::move(options));
  if (!gateway_result.has_value()) {
    std::cerr << "[spike] failed to create gateway: "
              << foxglove::strerror(gateway_result.error()) << '\n';
    std::cerr << "[spike] is FOXGLOVE_DEVICE_TOKEN set?\n";
    return 1;
  }
  auto gateway = std::move(gateway_result.value());

  auto channel_result = foxglove::RawChannel::create("/spike/timestamp", "json");
  if (!channel_result.has_value()) {
    std::cerr << "[spike] failed to create channel: "
              << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channel_result.value());

  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "[spike] shutting down...\n";
    done = true;
  };

#ifdef SPIKE_WITH_ROS
  ros::init(argc, argv, "ros1_spike", ros::init_options::NoSigintHandler);
  ros::NodeHandle nh;

  // Created lazily from the first message's connection header, which carries the
  // datatype and full (gendeps-style) message definition — this is the property
  // that makes a ROS 1 bridge simpler than ROS 2: no message definition cache.
  std::optional<foxglove::RawChannel> ros_channel;

  boost::function<void(const topic_tools::ShapeShifter::ConstPtr&)> ros_cb =
    [&](const topic_tools::ShapeShifter::ConstPtr& msg) {
      if (!ros_channel) {
        const std::string& definition = msg->getMessageDefinition();
        foxglove::Schema schema;
        schema.name = msg->getDataType();
        schema.encoding = "ros1msg";
        schema.data = reinterpret_cast<const std::byte*>(definition.data());
        schema.data_len = definition.size();
        auto result = foxglove::RawChannel::create("/chatter", "ros1", schema);
        if (!result.has_value()) {
          std::cerr << "[spike] failed to create ros channel: "
                    << foxglove::strerror(result.error()) << '\n';
          return;
        }
        std::cerr << "[spike] advertised /chatter as " << msg->getDataType()
                  << " (md5: " << msg->getMD5Sum() << ")\n";
        ros_channel.emplace(std::move(result.value()));
      }

      std::vector<uint8_t> buf(msg->size());
      ros::serialization::OStream stream(buf.data(), static_cast<uint32_t>(buf.size()));
      msg->write(stream);
      auto now = static_cast<uint64_t>(
        std::chrono::nanoseconds(std::chrono::system_clock::now().time_since_epoch()).count()
      );
      ros_channel->log(reinterpret_cast<const std::byte*>(buf.data()), buf.size(), now);
    };
  ros::Subscriber sub = nh.subscribe("/chatter", 10, ros_cb);
  std::cerr << "[spike] subscribed to /chatter via ShapeShifter\n";
#endif

  while (!done) {
#ifdef SPIKE_WITH_ROS
    if (!ros::ok()) {
      break;
    }
    ros::spinOnce();
#endif
    std::this_thread::sleep_for(100ms);
    auto now = static_cast<uint64_t>(
      std::chrono::nanoseconds(std::chrono::system_clock::now().time_since_epoch()).count()
    );
    std::string msg = "{\"timestamp\":" + std::to_string(now) + "}";
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size(), now);
  }

  gateway.stop();
  std::cerr << "[spike] done\n";
  return 0;
}
