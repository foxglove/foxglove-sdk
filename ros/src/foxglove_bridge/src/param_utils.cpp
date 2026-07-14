#include "foxglove_bridge/param_utils.hpp"

#include <rcl_interfaces/msg/parameter_descriptor.hpp>

#include <foxglove_bridge/common.hpp>

namespace foxglove_bridge {

void declareParameters(rclcpp::Node* node) {
  auto portDescription = rcl_interfaces::msg::ParameterDescriptor{};
  portDescription.name = PARAM_PORT;
  portDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER;
  portDescription.description = "The TCP port to bind the WebSocket server to";
  portDescription.read_only = true;
  portDescription.additional_constraints =
    "Must be a valid TCP port number, or 0 to use a random port";
  portDescription.integer_range.resize(1);
  portDescription.integer_range[0].from_value = 0;
  portDescription.integer_range[0].to_value = 65535;
  portDescription.integer_range[0].step = 1;
  node->declare_parameter(PARAM_PORT, DEFAULT_PORT, portDescription);

  auto debugDescription = rcl_interfaces::msg::ParameterDescriptor{};
  debugDescription.name = PARAM_DEBUG;
  debugDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  debugDescription.description = "Enable debug logging";
  debugDescription.read_only = true;
  node->declare_parameter(PARAM_DEBUG, false, debugDescription);

  auto addressDescription = rcl_interfaces::msg::ParameterDescriptor{};
  addressDescription.name = PARAM_ADDRESS;
  addressDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
  addressDescription.description = "The host address to bind the WebSocket server to";
  addressDescription.read_only = true;
  node->declare_parameter(PARAM_ADDRESS, DEFAULT_ADDRESS, addressDescription);

  auto sendBufferLimitDescription = rcl_interfaces::msg::ParameterDescriptor{};
  sendBufferLimitDescription.name = PARAM_SEND_BUFFER_LIMIT;
  sendBufferLimitDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER;
  sendBufferLimitDescription.description =
    "Connection send buffer limit in bytes. Messages will be dropped when a connection's send "
    "buffer reaches this limit to avoid a queue of outdated messages building up.";
  sendBufferLimitDescription.integer_range.resize(1);
  sendBufferLimitDescription.integer_range[0].from_value = 0;
  sendBufferLimitDescription.integer_range[0].to_value = std::numeric_limits<int64_t>::max();
  sendBufferLimitDescription.read_only = true;
  node->declare_parameter(PARAM_SEND_BUFFER_LIMIT, DEFAULT_SEND_BUFFER_LIMIT,
                          sendBufferLimitDescription);

  auto useTlsDescription = rcl_interfaces::msg::ParameterDescriptor{};
  useTlsDescription.name = PARAM_USETLS;
  useTlsDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  useTlsDescription.description = "Use Transport Layer Security for encrypted communication";
  useTlsDescription.read_only = true;
  node->declare_parameter(PARAM_USETLS, false, useTlsDescription);

  auto certfileDescription = rcl_interfaces::msg::ParameterDescriptor{};
  certfileDescription.name = PARAM_CERTFILE;
  certfileDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
  certfileDescription.description = "Path to the certificate to use for TLS";
  certfileDescription.read_only = true;
  node->declare_parameter(PARAM_CERTFILE, "", certfileDescription);

  auto keyfileDescription = rcl_interfaces::msg::ParameterDescriptor{};
  keyfileDescription.name = PARAM_KEYFILE;
  keyfileDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
  keyfileDescription.description = "Path to the private key to use for TLS";
  keyfileDescription.read_only = true;
  node->declare_parameter(PARAM_KEYFILE, "", keyfileDescription);

  auto minQosDepthDescription = rcl_interfaces::msg::ParameterDescriptor{};
  minQosDepthDescription.name = PARAM_MIN_QOS_DEPTH;
  minQosDepthDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER;
  minQosDepthDescription.description = "Minimum depth used for the QoS profile of subscriptions.";
  minQosDepthDescription.read_only = true;
  minQosDepthDescription.additional_constraints = "Must be a non-negative integer";
  minQosDepthDescription.integer_range.resize(1);
  minQosDepthDescription.integer_range[0].from_value = 0;
  minQosDepthDescription.integer_range[0].to_value = INT32_MAX;
  minQosDepthDescription.integer_range[0].step = 1;
  node->declare_parameter(PARAM_MIN_QOS_DEPTH, DEFAULT_MIN_QOS_DEPTH, minQosDepthDescription);

  auto maxQosDepthDescription = rcl_interfaces::msg::ParameterDescriptor{};
  maxQosDepthDescription.name = PARAM_MAX_QOS_DEPTH;
  maxQosDepthDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER;
  maxQosDepthDescription.description = "Maximum depth used for the QoS profile of subscriptions.";
  maxQosDepthDescription.read_only = true;
  maxQosDepthDescription.additional_constraints = "Must be a non-negative integer";
  maxQosDepthDescription.integer_range.resize(1);
  maxQosDepthDescription.integer_range[0].from_value = 0;
  maxQosDepthDescription.integer_range[0].to_value = INT32_MAX;
  maxQosDepthDescription.integer_range[0].step = 1;
  node->declare_parameter(PARAM_MAX_QOS_DEPTH, DEFAULT_MAX_QOS_DEPTH, maxQosDepthDescription);

  auto bestEffortQosTopicAllowlistDescription = rcl_interfaces::msg::ParameterDescriptor{};
  bestEffortQosTopicAllowlistDescription.name = PARAM_BEST_EFFORT_QOS_TOPIC_ALLOWLIST;
  bestEffortQosTopicAllowlistDescription.type =
    rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  bestEffortQosTopicAllowlistDescription.description =
    "List of regular expressions (ECMAScript) for topics that should be forced to use "
    "'best_effort' QoS. Unmatched topics will use 'reliable' QoS if ALL publishers are 'reliable', "
    "'best_effort' if any publishers are 'best_effort'.";
  bestEffortQosTopicAllowlistDescription.read_only = true;
  node->declare_parameter(PARAM_BEST_EFFORT_QOS_TOPIC_ALLOWLIST, std::vector<std::string>({"(?!)"}),
                          bestEffortQosTopicAllowlistDescription);

  auto topicAllowlistDescription = rcl_interfaces::msg::ParameterDescriptor{};
  topicAllowlistDescription.name = PARAM_TOPIC_ALLOWLIST;
  topicAllowlistDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  topicAllowlistDescription.description =
    "List of regular expressions (ECMAScript) of allowed topic names.";
  topicAllowlistDescription.read_only = true;
  node->declare_parameter(PARAM_TOPIC_ALLOWLIST, std::vector<std::string>({".*"}),
                          topicAllowlistDescription);

  auto serviceAllowlistDescription = rcl_interfaces::msg::ParameterDescriptor{};
  serviceAllowlistDescription.name = PARAM_SERVICE_ALLOWLIST;
  serviceAllowlistDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  serviceAllowlistDescription.description =
    "List of regular expressions (ECMAScript) of allowed service names.";
  serviceAllowlistDescription.read_only = true;
  node->declare_parameter(PARAM_SERVICE_ALLOWLIST, std::vector<std::string>({".*"}),
                          serviceAllowlistDescription);

  auto paramAllowlistDescription = rcl_interfaces::msg::ParameterDescriptor{};
  paramAllowlistDescription.name = PARAM_PARAMETER_ALLOWLIST;
  paramAllowlistDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  paramAllowlistDescription.description =
    "List of regular expressions (ECMAScript) of allowed parameter names.";
  paramAllowlistDescription.read_only = true;
  node->declare_parameter(PARAM_PARAMETER_ALLOWLIST, std::vector<std::string>({".*"}),
                          paramAllowlistDescription);

  auto useCompressionDescription = rcl_interfaces::msg::ParameterDescriptor{};
  useCompressionDescription.name = PARAM_USE_COMPRESSION;
  useCompressionDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  useCompressionDescription.description =
    "Use websocket compression (permessage-deflate). Suited for connections with smaller bandwith, "
    "at the cost of additional CPU load.";
  useCompressionDescription.read_only = true;
  node->declare_parameter(PARAM_USE_COMPRESSION, false, useCompressionDescription);

  auto paramCapabilities = rcl_interfaces::msg::ParameterDescriptor{};
  paramCapabilities.name = PARAM_CAPABILITIES;
  paramCapabilities.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  paramCapabilities.description = "Server capabilities";
  paramCapabilities.read_only = true;
  node->declare_parameter(PARAM_CAPABILITIES,
                          std::vector<std::string>(std::vector<std::string>(
                            DEFAULT_CAPABILITIES.begin(), DEFAULT_CAPABILITIES.end())),
                          paramCapabilities);

  auto clientTopicAllowlistDescription = rcl_interfaces::msg::ParameterDescriptor{};
  clientTopicAllowlistDescription.name = PARAM_CLIENT_TOPIC_ALLOWLIST;
  clientTopicAllowlistDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  clientTopicAllowlistDescription.description =
    "List of regular expressions (ECMAScript) of allowed client-published topic names.";
  clientTopicAllowlistDescription.read_only = true;
  node->declare_parameter(PARAM_CLIENT_TOPIC_ALLOWLIST, std::vector<std::string>({".*"}),
                          paramAllowlistDescription);

  // Deprecated *_whitelist aliases for the *_allowlist parameters above. Declared with an
  // empty-array default that acts as an "unset" sentinel (see
  // getStringArrayParamWithDeprecatedAlias).
  auto declareDeprecatedAlias = [node](const char* deprecatedName, const char* canonicalName) {
    auto descriptor = rcl_interfaces::msg::ParameterDescriptor{};
    descriptor.name = deprecatedName;
    descriptor.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
    descriptor.description = "Deprecated: use '" + std::string(canonicalName) + "' instead.";
    descriptor.read_only = true;
    node->declare_parameter(deprecatedName, std::vector<std::string>{}, descriptor);
  };
  declareDeprecatedAlias(PARAM_BEST_EFFORT_QOS_TOPIC_ALLOWLIST_DEPRECATED,
                         PARAM_BEST_EFFORT_QOS_TOPIC_ALLOWLIST);
  declareDeprecatedAlias(PARAM_TOPIC_ALLOWLIST_DEPRECATED, PARAM_TOPIC_ALLOWLIST);
  declareDeprecatedAlias(PARAM_SERVICE_ALLOWLIST_DEPRECATED, PARAM_SERVICE_ALLOWLIST);
  declareDeprecatedAlias(PARAM_PARAMETER_ALLOWLIST_DEPRECATED, PARAM_PARAMETER_ALLOWLIST);
  declareDeprecatedAlias(PARAM_CLIENT_TOPIC_ALLOWLIST_DEPRECATED, PARAM_CLIENT_TOPIC_ALLOWLIST);

  auto includeHiddenDescription = rcl_interfaces::msg::ParameterDescriptor{};
  includeHiddenDescription.name = PARAM_INCLUDE_HIDDEN;
  includeHiddenDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  includeHiddenDescription.description = "Include hidden topics and services";
  includeHiddenDescription.read_only = true;
  node->declare_parameter(PARAM_INCLUDE_HIDDEN, false, includeHiddenDescription);

  auto disableLoanMessageDescription = rcl_interfaces::msg::ParameterDescriptor{};
  disableLoanMessageDescription.name = PARAM_DISABLE_LOAN_MESSAGE;
  disableLoanMessageDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  disableLoanMessageDescription.description =
    "Do not publish as loaned message when publishing a client message";
  disableLoanMessageDescription.read_only = true;
  node->declare_parameter(PARAM_DISABLE_LOAN_MESSAGE, true, disableLoanMessageDescription);

  auto assetUriAllowlistDescription = rcl_interfaces::msg::ParameterDescriptor{};
  assetUriAllowlistDescription.name = PARAM_ASSET_URI_ALLOWLIST;
  assetUriAllowlistDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  assetUriAllowlistDescription.description =
    "List of regular expressions (ECMAScript) of allowed asset URIs.";
  assetUriAllowlistDescription.read_only = true;
  node->declare_parameter(
    PARAM_ASSET_URI_ALLOWLIST,
    std::vector<std::string>(
      {"^package://(?:[-\\w%]+/"
       ")*[-\\w%.]+\\.(?:dae|fbx|glb|gltf|jpeg|jpg|mtl|obj|png|stl|tif|tiff|urdf|webp|xacro)$"}),
    paramAllowlistDescription);

  auto ignUnresponsiveParamNodes = rcl_interfaces::msg::ParameterDescriptor{};
  ignUnresponsiveParamNodes.name = PARAM_IGN_UNRESPONSIVE_PARAM_NODES;
  ignUnresponsiveParamNodes.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  ignUnresponsiveParamNodes.description =
    "Avoid requesting parameters from previously unresponsive nodes";
  ignUnresponsiveParamNodes.read_only = true;
  node->declare_parameter(PARAM_IGN_UNRESPONSIVE_PARAM_NODES, true, ignUnresponsiveParamNodes);

  auto publishClientCountDescription = rcl_interfaces::msg::ParameterDescriptor{};
  publishClientCountDescription.name = PARAM_PUBLISH_CLIENT_COUNT;
  publishClientCountDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  publishClientCountDescription.description = "Publish the number of connected clients";
  publishClientCountDescription.read_only = true;
  node->declare_parameter(PARAM_PUBLISH_CLIENT_COUNT, false, publishClientCountDescription);

  auto sysinfoDescription = rcl_interfaces::msg::ParameterDescriptor{};
  sysinfoDescription.name = PARAM_SYSINFO;
  sysinfoDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  sysinfoDescription.description =
    "Publish process and system statistics (CPU, memory, etc.) on a channel";
  sysinfoDescription.read_only = true;
  node->declare_parameter(PARAM_SYSINFO, true, sysinfoDescription);

  auto sysinfoTopicDescription = rcl_interfaces::msg::ParameterDescriptor{};
  sysinfoTopicDescription.name = PARAM_SYSINFO_TOPIC;
  sysinfoTopicDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
  sysinfoTopicDescription.description =
    "Topic name for system info messages. Defaults to /foxglove_bridge/sysinfo.";
  sysinfoTopicDescription.read_only = true;
  node->declare_parameter(PARAM_SYSINFO_TOPIC, DEFAULT_SYSINFO_TOPIC, sysinfoTopicDescription);

  auto sysinfoRefreshIntervalDescription = rcl_interfaces::msg::ParameterDescriptor{};
  sysinfoRefreshIntervalDescription.name = PARAM_SYSINFO_REFRESH_INTERVAL;
  sysinfoRefreshIntervalDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER;
  sysinfoRefreshIntervalDescription.description =
    "Refresh interval for system info messages in milliseconds. Minimum 200ms.";
  sysinfoRefreshIntervalDescription.read_only = true;
  sysinfoRefreshIntervalDescription.integer_range.resize(1);
  sysinfoRefreshIntervalDescription.integer_range[0].from_value = 200;
  sysinfoRefreshIntervalDescription.integer_range[0].to_value = std::numeric_limits<int64_t>::max();
  sysinfoRefreshIntervalDescription.integer_range[0].step = 1;
  node->declare_parameter(PARAM_SYSINFO_REFRESH_INTERVAL, DEFAULT_SYSINFO_REFRESH_INTERVAL_MS,
                          sysinfoRefreshIntervalDescription);

  auto messageBacklogSizeDescription = rcl_interfaces::msg::ParameterDescriptor{};
  messageBacklogSizeDescription.name = PARAM_MESSAGE_BACKLOG_SIZE;
  messageBacklogSizeDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER;
  messageBacklogSizeDescription.description =
    "Maximum number of outgoing messages to buffer per connected WebSocket client or "
    "remote access gateway participant. The WebSocket server drops the oldest data-plane "
    "message on overflow and disconnects clients whose control-plane queue fills. The "
    "remote access gateway disconnects participants whose queue fills.";
  messageBacklogSizeDescription.read_only = true;
  messageBacklogSizeDescription.integer_range.resize(1);
  messageBacklogSizeDescription.integer_range[0].from_value = 1;
  messageBacklogSizeDescription.integer_range[0].to_value = std::numeric_limits<int64_t>::max();
  messageBacklogSizeDescription.integer_range[0].step = 1;
  node->declare_parameter(PARAM_MESSAGE_BACKLOG_SIZE, DEFAULT_MESSAGE_BACKLOG_SIZE,
                          messageBacklogSizeDescription);

  auto remoteAccessDescription = rcl_interfaces::msg::ParameterDescriptor{};
  remoteAccessDescription.name = PARAM_REMOTE_ACCESS;
  remoteAccessDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_BOOL;
  remoteAccessDescription.description =
    "Enable the remote access gateway for Foxglove platform. "
    "Requires the bridge to be built with FOXGLOVE_BRIDGE_REMOTE_ACCESS=ON.";
  remoteAccessDescription.read_only = true;
  node->declare_parameter(PARAM_REMOTE_ACCESS, false, remoteAccessDescription);

  auto deviceTokenDescription = rcl_interfaces::msg::ParameterDescriptor{};
  deviceTokenDescription.name = PARAM_DEVICE_TOKEN;
  deviceTokenDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
  deviceTokenDescription.description =
    "Foxglove device token for platform authentication. "
    "If empty, falls back to FOXGLOVE_DEVICE_TOKEN environment variable.";
  deviceTokenDescription.read_only = true;
  node->declare_parameter(PARAM_DEVICE_TOKEN, "", deviceTokenDescription);

  auto foxgloveApiUrlDescription = rcl_interfaces::msg::ParameterDescriptor{};
  foxgloveApiUrlDescription.name = PARAM_FOXGLOVE_API_URL;
  foxgloveApiUrlDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
  foxgloveApiUrlDescription.description =
    "Override the Foxglove API base URL. If empty, the SDK default is used.";
  foxgloveApiUrlDescription.read_only = true;
  node->declare_parameter(PARAM_FOXGLOVE_API_URL, "", foxgloveApiUrlDescription);

  auto videoEncoderDescription = rcl_interfaces::msg::ParameterDescriptor{};
  videoEncoderDescription.name = PARAM_VIDEO_ENCODER;
  videoEncoderDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
  videoEncoderDescription.description =
    "Preferred backend for encoding published video tracks: one of 'auto', 'software', "
    "'hardware', 'nvenc', 'vaapi', 'videotoolbox'. With 'auto' (the default) the SDK chooses, "
    "and honors the FOXGLOVE_VIDEO_ENCODER environment variable. If the requested backend is "
    "unavailable, the SDK falls back to another compatible encoder.";
  videoEncoderDescription.read_only = true;
  node->declare_parameter(PARAM_VIDEO_ENCODER, "auto", videoEncoderDescription);

  auto maxDataTrackMessageSizeDescription = rcl_interfaces::msg::ParameterDescriptor{};
  maxDataTrackMessageSizeDescription.name = PARAM_MAX_DATA_TRACK_MESSAGE_SIZE;
  maxDataTrackMessageSizeDescription.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER;
  maxDataTrackMessageSizeDescription.description =
    "Maximum size, in bytes, of a lossy data-track message sent by the remote access gateway. "
    "Larger messages are dropped before publishing, with a throttled warning, so one "
    "high-bandwidth channel cannot starve the others. Must be at least 1200 (one data-channel "
    "packet).";
  maxDataTrackMessageSizeDescription.read_only = true;
  maxDataTrackMessageSizeDescription.integer_range.resize(1);
  maxDataTrackMessageSizeDescription.integer_range[0].from_value = 1200;
  maxDataTrackMessageSizeDescription.integer_range[0].to_value =
    std::numeric_limits<int64_t>::max();
  maxDataTrackMessageSizeDescription.integer_range[0].step = 1;
  node->declare_parameter(PARAM_MAX_DATA_TRACK_MESSAGE_SIZE, DEFAULT_MAX_DATA_TRACK_MESSAGE_SIZE,
                          maxDataTrackMessageSizeDescription);

  auto videoTranscodeTopicDenylistDescription = rcl_interfaces::msg::ParameterDescriptor{};
  videoTranscodeTopicDenylistDescription.name = PARAM_VIDEO_TRANSCODE_TOPIC_DENYLIST;
  videoTranscodeTopicDenylistDescription.type =
    rcl_interfaces::msg::ParameterType::PARAMETER_STRING_ARRAY;
  videoTranscodeTopicDenylistDescription.description =
    "List of regular expressions (ECMAScript) of topic names delivered as data over remote access "
    "instead of being transcoded to video. Use this for image topics whose pixel values must not "
    "pass through lossy video, such as compressed depth maps. Defaults to match the "
    "'compressed_depth_image_transport' '/compressedDepth' suffix.";
  videoTranscodeTopicDenylistDescription.read_only = true;
  node->declare_parameter(PARAM_VIDEO_TRANSCODE_TOPIC_DENYLIST,
                          std::vector<std::string>({DEFAULT_VIDEO_TRANSCODE_TOPIC_DENYLIST}),
                          videoTranscodeTopicDenylistDescription);
}

std::vector<std::regex> parseRegexStrings(rclcpp::Node* node,
                                          const std::vector<std::string>& strings) {
  std::vector<std::regex> regexVector;
  regexVector.reserve(strings.size());

  for (const auto& pattern : strings) {
    try {
      regexVector.push_back(compileTopicRegex(pattern));
    } catch (const std::exception& ex) {
      RCLCPP_ERROR(node->get_logger(), "Ignoring invalid regular expression '%s': %s",
                   pattern.c_str(), ex.what());
    }
  }

  return regexVector;
}

std::vector<std::string> resolveAliasedStringArray(const std::vector<std::string>& canonical,
                                                   const std::vector<std::string>& deprecated,
                                                   bool& usedDeprecated) {
  usedDeprecated = !deprecated.empty();
  return usedDeprecated ? deprecated : canonical;
}

std::vector<std::string> getStringArrayParamWithDeprecatedAlias(rclcpp::Node* node,
                                                                const std::string& canonicalName,
                                                                const std::string& deprecatedName) {
  bool usedDeprecated = false;
  auto value = resolveAliasedStringArray(node->get_parameter(canonicalName).as_string_array(),
                                         node->get_parameter(deprecatedName).as_string_array(),
                                         usedDeprecated);
  if (usedDeprecated) {
    RCLCPP_WARN(node->get_logger(), "Parameter '%s' is deprecated; use '%s' instead.",
                deprecatedName.c_str(), canonicalName.c_str());
  }
  return value;
}

}  // namespace foxglove_bridge
