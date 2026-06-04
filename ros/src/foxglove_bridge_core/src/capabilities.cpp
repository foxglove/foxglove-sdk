#include <unordered_map>

#include <foxglove_bridge_core/capabilities.hpp>

namespace foxglove_bridge {

foxglove::WebSocketServerCapabilities processCapabilities(
  const std::vector<std::string>& capabilities) {
  const std::unordered_map<std::string, foxglove::WebSocketServerCapabilities>
    STRING_TO_CAPABILITY = {
      {"clientPublish", foxglove::WebSocketServerCapabilities::ClientPublish},
      {"parameters", foxglove::WebSocketServerCapabilities::Parameters},
      {"parametersSubscribe", foxglove::WebSocketServerCapabilities::Parameters},
      {"services", foxglove::WebSocketServerCapabilities::Services},
      {"connectionGraph", foxglove::WebSocketServerCapabilities::ConnectionGraph},
      {"assets", foxglove::WebSocketServerCapabilities::Assets},
    };
  foxglove::WebSocketServerCapabilities out = foxglove::WebSocketServerCapabilities::None;
  for (const auto& capability : capabilities) {
    if (STRING_TO_CAPABILITY.find(capability) != STRING_TO_CAPABILITY.end()) {
      out = out | STRING_TO_CAPABILITY.at(capability);
    }
  }
  return out;
}

#ifdef FOXGLOVE_REMOTE_ACCESS
foxglove::RemoteAccessGatewayCapabilities toGatewayCapabilities(
  foxglove::WebSocketServerCapabilities capabilities) {
  foxglove::RemoteAccessGatewayCapabilities out =
    foxglove::RemoteAccessGatewayCapabilities::None;
  if (hasCapability(capabilities, foxglove::WebSocketServerCapabilities::ClientPublish)) {
    out = out | foxglove::RemoteAccessGatewayCapabilities::ClientPublish;
  }
  if (hasCapability(capabilities, foxglove::WebSocketServerCapabilities::Parameters)) {
    out = out | foxglove::RemoteAccessGatewayCapabilities::Parameters;
  }
  if (hasCapability(capabilities, foxglove::WebSocketServerCapabilities::Services)) {
    out = out | foxglove::RemoteAccessGatewayCapabilities::Services;
  }
  if (hasCapability(capabilities, foxglove::WebSocketServerCapabilities::Assets)) {
    out = out | foxglove::RemoteAccessGatewayCapabilities::Assets;
  }
  if (hasCapability(capabilities, foxglove::WebSocketServerCapabilities::ConnectionGraph)) {
    out = out | foxglove::RemoteAccessGatewayCapabilities::ConnectionGraph;
  }
  return out;
}
#endif

}  // namespace foxglove_bridge
