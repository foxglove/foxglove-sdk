#pragma once

#include <regex>
#include <string>
#include <vector>

#include <rclcpp/node.hpp>

namespace foxglove_bridge {

constexpr char PARAM_PORT[] = "port";
constexpr char PARAM_DEBUG[] = "debug";
constexpr char PARAM_ADDRESS[] = "address";
constexpr char PARAM_SEND_BUFFER_LIMIT[] = "send_buffer_limit";
constexpr char PARAM_USETLS[] = "tls";
constexpr char PARAM_CERTFILE[] = "certfile";
constexpr char PARAM_KEYFILE[] = "keyfile";
constexpr char PARAM_MIN_QOS_DEPTH[] = "min_qos_depth";
constexpr char PARAM_MAX_QOS_DEPTH[] = "max_qos_depth";
constexpr char PARAM_BEST_EFFORT_QOS_TOPIC_ALLOWLIST[] = "best_effort_qos_topic_allowlist";
constexpr char PARAM_TOPIC_ALLOWLIST[] = "topic_allowlist";
constexpr char PARAM_SERVICE_ALLOWLIST[] = "service_allowlist";
constexpr char PARAM_PARAMETER_ALLOWLIST[] = "param_allowlist";
constexpr char PARAM_USE_COMPRESSION[] = "use_compression";
constexpr char PARAM_CAPABILITIES[] = "capabilities";
constexpr char PARAM_CLIENT_TOPIC_ALLOWLIST[] = "client_topic_allowlist";
constexpr char PARAM_INCLUDE_HIDDEN[] = "include_hidden";
constexpr char PARAM_DISABLE_LOAN_MESSAGE[] = "disable_load_message";
constexpr char PARAM_ASSET_URI_ALLOWLIST[] = "asset_uri_allowlist";
constexpr char PARAM_IGN_UNRESPONSIVE_PARAM_NODES[] = "ignore_unresponsive_param_nodes";
constexpr char PARAM_PUBLISH_CLIENT_COUNT[] = "publish_client_count";
constexpr char PARAM_SYSINFO[] = "sysinfo";
constexpr char PARAM_SYSINFO_TOPIC[] = "sysinfo_topic";
constexpr char PARAM_SYSINFO_REFRESH_INTERVAL[] = "sysinfo_refresh_interval";
constexpr char PARAM_MESSAGE_BACKLOG_SIZE[] = "message_backlog_size";

constexpr char PARAM_REMOTE_ACCESS[] = "remote_access";
constexpr char PARAM_DEVICE_TOKEN[] = "device_token";
constexpr char PARAM_FOXGLOVE_API_URL[] = "foxglove_api_url";
constexpr char PARAM_VIDEO_ENCODER[] = "video_encoder";
constexpr char PARAM_MAX_DATA_TRACK_MESSAGE_SIZE[] = "max_data_track_message_size";
constexpr char PARAM_VIDEO_TRANSCODE_TOPIC_DENYLIST[] = "video_transcode_topic_denylist";

// Deprecated names for the *_allowlist parameters above. Not declared as parameters themselves;
// see getStringArrayParamWithDeprecatedAlias. Removal tracked in FLE-672.
constexpr char PARAM_BEST_EFFORT_QOS_TOPIC_ALLOWLIST_DEPRECATED[] =
  "best_effort_qos_topic_whitelist";
constexpr char PARAM_TOPIC_ALLOWLIST_DEPRECATED[] = "topic_whitelist";
constexpr char PARAM_SERVICE_ALLOWLIST_DEPRECATED[] = "service_whitelist";
constexpr char PARAM_PARAMETER_ALLOWLIST_DEPRECATED[] = "param_whitelist";
constexpr char PARAM_CLIENT_TOPIC_ALLOWLIST_DEPRECATED[] = "client_topic_whitelist";

constexpr int64_t DEFAULT_PORT = 8765;
constexpr char DEFAULT_ADDRESS[] = "0.0.0.0";
constexpr int64_t DEFAULT_SEND_BUFFER_LIMIT = 10000000;
constexpr int64_t DEFAULT_MIN_QOS_DEPTH = 1;
constexpr int64_t DEFAULT_MAX_QOS_DEPTH = 25;
constexpr char DEFAULT_SYSINFO_TOPIC[] = "/foxglove_bridge/sysinfo";
constexpr int64_t DEFAULT_SYSINFO_REFRESH_INTERVAL_MS = 500;
constexpr int64_t DEFAULT_MESSAGE_BACKLOG_SIZE = 1024;
constexpr int64_t DEFAULT_MAX_DATA_TRACK_MESSAGE_SIZE = 102400;
constexpr char DEFAULT_VIDEO_TRANSCODE_TOPIC_DENYLIST[] = ".*/compressedDepth";

/// Regex matching nothing. Used to disable a topic-list parameter, since ROS can't take `[]` as
/// an override value.
constexpr char REGEX_MATCH_NOTHING[] = "(?!)";

void declareParameters(rclcpp::Node* node);

/// Compiles a topic-matching regex with the flags the bridge applies to every topic pattern
/// (ECMAScript, case-insensitive). Shared by parseRegexStrings and tests so both exercise the
/// same regex behavior.
inline std::regex compileTopicRegex(const std::string& pattern) {
  return std::regex(pattern, std::regex_constants::ECMAScript | std::regex_constants::icase);
}

std::vector<std::regex> parseRegexStrings(rclcpp::Node* node,
                                          const std::vector<std::string>& strings);

/// Reads canonicalName. Falls back to deprecatedName's override (logging a deprecation warning)
/// only if canonicalName has no override of its own; an explicit canonicalName override always
/// wins over deprecatedName.
std::vector<std::string> getStringArrayParamWithDeprecatedAlias(rclcpp::Node* node,
                                                                const std::string& canonicalName,
                                                                const std::string& deprecatedName);

}  // namespace foxglove_bridge
