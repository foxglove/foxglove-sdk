#include <stdexcept>
#include <string>

// Builds this plugin as if against another foxglove_bridge version.
#undef FOXGLOVE_BRIDGE_PLUGIN_ABI_NAMESPACE
#define FOXGLOVE_BRIDGE_PLUGIN_ABI_NAMESPACE abi_v0_0_0

#include <pluginlib/class_list_macros.hpp>
#include <rclcpp/rclcpp.hpp>

#include <foxglove_bridge/channel_source_plugin.hpp>

namespace foxglove_bridge_test_plugins {

class VersionMismatchPlugin : public foxglove_bridge::ChannelSourcePlugin {
public:
  VersionMismatchPlugin() {
    throw std::logic_error("constructed despite the version mismatch");
  }

  void initialize(rclcpp::Node& /*node*/, const std::string& /*name*/,
                  const foxglove::Context& /*context*/) override {}

  void shutdown() override {}
};

}  // namespace foxglove_bridge_test_plugins

PLUGINLIB_EXPORT_CLASS(foxglove_bridge_test_plugins::VersionMismatchPlugin,
                       foxglove_bridge::ChannelSourcePlugin)
