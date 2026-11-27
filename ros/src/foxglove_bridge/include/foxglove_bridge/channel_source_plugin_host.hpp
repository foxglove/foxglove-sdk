#pragma once

#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <pluginlib/class_loader.hpp>
#include <rclcpp/node.hpp>

#include <foxglove/context.hpp>
#include <foxglove_bridge/channel_source_plugin.hpp>

namespace foxglove_bridge {

/// Loads the channel source plugin instances of a bridge node.
class ChannelSourcePluginHost {
public:
  ChannelSourcePluginHost(rclcpp::Node& node, const foxglove::Context& context);

  /// Loads the named instances. Failing instances are logged and skipped.
  void load(const std::vector<std::string>& instanceNames);

  /// Shuts the loaded plugins down in reverse load order.
  void shutdown();

private:
  void loadInstance(const std::string& name);

  rclcpp::Node& _node;
  const foxglove::Context& _context;
  // Declared before the plugins, which it must outlive.
  std::optional<pluginlib::ClassLoader<ChannelSourcePlugin>> _loader;
  std::vector<pluginlib::UniquePtr<ChannelSourcePlugin>> _plugins;
};

}  // namespace foxglove_bridge
