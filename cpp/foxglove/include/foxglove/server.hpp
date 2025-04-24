#pragma once

#include <foxglove/error.hpp>
#include <foxglove/server/connection_graph.hpp>
#include <foxglove/server/parameter.hpp>

#include <cstdint>
#include <functional>
#include <memory>
#include <string>

enum foxglove_error : uint8_t;
struct foxglove_websocket_server;
struct foxglove_connection_graph;

namespace foxglove {

struct ClientChannel {
  uint32_t id;
  std::string_view topic;
  std::string_view encoding;
  std::string_view schemaName;
  std::string_view schemaEncoding;
  const std::byte* schema;
  size_t schemaLen;
};

enum class WebSocketServerCapabilities : uint8_t {
  /// Allow clients to advertise channels to send data messages to the server.
  ClientPublish = 1 << 0,
  /// Allow clients to subscribe and make connection graph updates
  ConnectionGraph = 1 << 1,
  /// Allow clients to get & set parameters.
  Parameters = 1 << 2,
  /// Inform clients about the latest server time.
  ///
  /// This allows accelerated, slowed, or stepped control over the progress of time. If the
  /// server publishes time data, then timestamps of published messages must originate from the
  /// same time source.
  Time = 1 << 3,
  /// Allow clients to call services.
  Services = 1 << 4,
};

inline WebSocketServerCapabilities operator|(
  WebSocketServerCapabilities a, WebSocketServerCapabilities b
) {
  return WebSocketServerCapabilities(uint8_t(a) | uint8_t(b));
}

inline WebSocketServerCapabilities operator&(
  WebSocketServerCapabilities a, WebSocketServerCapabilities b
) {
  return WebSocketServerCapabilities(uint8_t(a) & uint8_t(b));
}

struct WebSocketServerCallbacks {
  std::function<void(uint64_t channelId)> onSubscribe;
  std::function<void(uint64_t channelId)> onUnsubscribe;
  std::function<void(uint32_t clientId, const ClientChannel& channel)> onClientAdvertise;
  std::function<
    void(uint32_t clientId, uint32_t clientChannelId, const std::byte* data, size_t dataLen)>
    onMessageData;
  std::function<void(uint32_t clientId, uint32_t clientChannelId)> onClientUnadvertise;
  std::function<std::vector<Parameter>(
    uint32_t clientId, std::string_view request_id, const std::vector<std::string>& param_names
  )>
    onGetParameters;
  std::function<std::vector<Parameter>(
    uint32_t clientId, std::string_view request_id, const std::vector<ParameterView>& params
  )>
    onSetParameters;
  std::function<void()> onConnectionGraphSubscribe;
  std::function<void()> onConnectionGraphUnsubscribe;
};

struct WebSocketServerOptions {
  std::string name;
  std::string host;
  uint16_t port;
  WebSocketServerCallbacks callbacks;
  WebSocketServerCapabilities capabilities = WebSocketServerCapabilities(0);
  std::vector<std::string> supportedEncodings;
};

class WebSocketServer final {
public:
  static FoxgloveResult<WebSocketServer> create(WebSocketServerOptions&& options);

  // Get the port on which the server is listening.
  uint16_t port() const;

  FoxgloveError stop();

  void publishConnectionGraph(ConnectionGraph& graph);

private:
  WebSocketServer(
    foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks
  );

  std::unique_ptr<WebSocketServerCallbacks> _callbacks;
  std::unique_ptr<foxglove_websocket_server, foxglove_error (*)(foxglove_websocket_server*)> _impl;
};

}  // namespace foxglove
