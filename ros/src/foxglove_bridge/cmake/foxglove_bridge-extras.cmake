# Defines foxglove_bridge::plugin_api for channel source plugins.
if(NOT TARGET foxglove_bridge::plugin_api)
  find_package(rclcpp REQUIRED)
  include("${foxglove_bridge_DIR}/foxglove_bridge_plugin_api.cmake")
endif()
