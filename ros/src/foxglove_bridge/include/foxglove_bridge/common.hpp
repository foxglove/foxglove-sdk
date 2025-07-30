#pragma once

#include <array>
#include <cstring>
#include <optional>
#include <regex>
#include <stdint.h>
#include <string>
#include <vector>

namespace foxglove_bridge {
constexpr char SUPPORTED_SUBPROTOCOL[] = "foxglove.sdk.v1";

constexpr char CAPABILITY_CLIENT_PUBLISH[] = "clientPublish";
constexpr char CAPABILITY_TIME[] = "time";
constexpr char CAPABILITY_PARAMETERS[] = "parameters";
constexpr char CAPABILITY_PARAMETERS_SUBSCRIBE[] = "parametersSubscribe";
constexpr char CAPABILITY_SERVICES[] = "services";
constexpr char CAPABILITY_CONNECTION_GRAPH[] = "connectionGraph";
constexpr char CAPABILITY_ASSETS[] = "assets";
constexpr std::array<const char*, 6> DEFAULT_CAPABILITIES = {
  CAPABILITY_CLIENT_PUBLISH, CAPABILITY_CONNECTION_GRAPH, CAPABILITY_PARAMETERS_SUBSCRIBE,
  CAPABILITY_PARAMETERS,     CAPABILITY_SERVICES,         CAPABILITY_ASSETS,
};
}  // namespace foxglove_bridge
