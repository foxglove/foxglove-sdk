#include <chrono>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>

#include <pluginlib/class_list_macros.hpp>
#include <rclcpp/rclcpp.hpp>

#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove_bridge/channel_source_plugin.hpp>

namespace foxglove_bridge_test_plugins {

constexpr std::string_view TEST_PLUGIN_MESSAGE = R"({"value":42})";
constexpr std::string_view TEST_PLUGIN_SCHEMA = R"({"type":"object"})";

/// Serves one JSON channel on "/<instance name>/data", logging a fixed message while a client is
/// subscribed.
class TestChannelSourcePlugin : public foxglove_bridge::ChannelSourcePlugin {
public:
  void initialize(rclcpp::Node& node, const std::string& name,
                  const foxglove::Context& context) override {
    const foxglove::Schema schema{"test_plugin.Data", "jsonschema",
                                  reinterpret_cast<const std::byte*>(TEST_PLUGIN_SCHEMA.data()),
                                  TEST_PLUGIN_SCHEMA.size()};
    auto channel = foxglove::RawChannel::create("/" + name + "/data", "json", schema, context);
    if (!channel.has_value()) {
      throw std::runtime_error(std::string("failed to create channel: ") +
                               foxglove::strerror(channel.error()));
    }
    _channel.emplace(std::move(channel.value()));
    _name = name;
    _logger = node.get_logger().get_child(name);

    _timer = node.create_wall_timer(std::chrono::milliseconds(20), [this, &node]() {
      if (_channel->hasSinks()) {
        _channel->log(reinterpret_cast<const std::byte*>(TEST_PLUGIN_MESSAGE.data()),
                      TEST_PLUGIN_MESSAGE.size(), static_cast<uint64_t>(node.now().nanoseconds()));
      }
    });
  }

  void shutdown() override {
    _timer->cancel();
    RCLCPP_INFO(_logger, "Test plugin instance '%s' shut down", _name.c_str());
  }

private:
  std::optional<foxglove::RawChannel> _channel;
  rclcpp::TimerBase::SharedPtr _timer;
  std::string _name;
  rclcpp::Logger _logger = rclcpp::get_logger("test_plugin");
};

class ThrowingPlugin : public foxglove_bridge::ChannelSourcePlugin {
public:
  void initialize(rclcpp::Node& /*node*/, const std::string& /*name*/,
                  const foxglove::Context& /*context*/) override {
    throw std::runtime_error("initialization failure requested by test");
  }

  void shutdown() override {}
};

}  // namespace foxglove_bridge_test_plugins

PLUGINLIB_EXPORT_CLASS(foxglove_bridge_test_plugins::TestChannelSourcePlugin,
                       foxglove_bridge::ChannelSourcePlugin)
PLUGINLIB_EXPORT_CLASS(foxglove_bridge_test_plugins::ThrowingPlugin,
                       foxglove_bridge::ChannelSourcePlugin)
