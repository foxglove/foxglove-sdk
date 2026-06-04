#pragma once

#include <string>
#include <vector>

#include <foxglove/websocket.hpp>
#ifdef FOXGLOVE_REMOTE_ACCESS
#include <foxglove/remote_access.hpp>
#endif

namespace foxglove_bridge {

/// Map capability names (as used in the `capabilities` bridge parameter) to
/// SDK WebSocket server capability flags. Unknown names are ignored.
foxglove::WebSocketServerCapabilities processCapabilities(
  const std::vector<std::string>& capabilities);

inline bool hasCapability(const foxglove::WebSocketServerCapabilities& capabilities,
                          foxglove::WebSocketServerCapabilities capability) {
  return (capabilities & capability) == capability;
}

#ifdef FOXGLOVE_REMOTE_ACCESS
/// Map WebSocket server capabilities to the equivalent remote access gateway
/// capabilities. (Time has no gateway equivalent and is dropped.)
foxglove::RemoteAccessGatewayCapabilities toGatewayCapabilities(
  foxglove::WebSocketServerCapabilities capabilities);
#endif

}  // namespace foxglove_bridge
