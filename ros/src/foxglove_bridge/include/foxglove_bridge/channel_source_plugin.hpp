#pragma once

#include <string>

#include <rclcpp/node.hpp>

#include <foxglove/context.hpp>

#ifndef FOXGLOVE_BRIDGE_PLUGIN_ABI_NAMESPACE
#error "Link channel source plugins against the foxglove_bridge::plugin_api CMake target"
#endif

namespace foxglove_bridge {

// Encodes the bridge version, so pluginlib can't create a plugin built against another version.
inline namespace FOXGLOVE_BRIDGE_PLUGIN_ABI_NAMESPACE {

/// Serves non-ROS channels over the bridge. Experimental: build plugins against the bridge version
/// that loads them. See the README.
class ChannelSourcePlugin {
public:
  virtual ~ChannelSourcePlugin() = default;

  ChannelSourcePlugin(const ChannelSourcePlugin&) = delete;
  ChannelSourcePlugin& operator=(const ChannelSourcePlugin&) = delete;
  ChannelSourcePlugin(ChannelSourcePlugin&&) = delete;
  ChannelSourcePlugin& operator=(ChannelSourcePlugin&&) = delete;

  /// Creates the plugin's channels on the bridge's `context`. Called from the bridge node's
  /// constructor, so `node.shared_from_this()` is not available yet. Read parameters under
  /// `<name>.` and timestamp messages with `node.now()`. A channel identical to a ROS topic's
  /// channel would be shared with the bridge.
  ///
  /// Throwing disables the plugin without calling shutdown(), so release what it created first.
  virtual void initialize(rclcpp::Node& node, const std::string& name,
                          const foxglove::Context& context) = 0;

  /// Stops all threads, callbacks and logging. Called before the server stops.
  virtual void shutdown() = 0;

protected:
  ChannelSourcePlugin() = default;
};

}  // namespace FOXGLOVE_BRIDGE_PLUGIN_ABI_NAMESPACE
}  // namespace foxglove_bridge
