#pragma once

#include <foxglove/error.hpp>
#include <foxglove/server/connection_graph.hpp>

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>

enum foxglove_error : uint8_t;
struct foxglove_websocket_server;
struct foxglove_connection_graph;

namespace foxglove {

struct Context;

struct ClientChannel {
  uint32_t id;
  std::string_view topic;
  std::string_view encoding;
  std::string_view schema_name;
  std::string_view schema_encoding;
  const std::byte* schema;
  size_t schema_len;
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
  std::function<void(uint64_t channel_id)> onSubscribe;
  std::function<void(uint64_t channel_id)> onUnsubscribe;
  std::function<void(uint32_t client_id, const ClientChannel& channel)> onClientAdvertise;
  std::function<
    void(uint32_t client_id, uint32_t client_channel_id, const std::byte* data, size_t data_len)>
    onMessageData;
  std::function<void(uint32_t client_id, uint32_t client_channel_id)> onClientUnadvertise;
  std::function<void()> onConnectionGraphSubscribe;
  std::function<void()> onConnectionGraphUnsubscribe;
};

struct WebSocketServerOptions {
  friend class WebSocketServer;

  Context context;
  std::string name;
  std::string host = "127.0.0.1";
  uint16_t port = 8765;  // default foxglove WebSocket port
  WebSocketServerCallbacks callbacks;
  WebSocketServerCapabilities capabilities = WebSocketServerCapabilities(0);
  std::vector<std::string> supported_encodings;
};

class WebSocketServer final {
public:
  static FoxgloveResult<WebSocketServer> create(WebSocketServerOptions&& options);

  // Get the port on which the server is listening.
  [[nodiscard]] uint16_t port() const;

  FoxgloveError stop();

  void publishConnectionGraph(ConnectionGraph& graph);

private:
  WebSocketServer(
    foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks
  );

  std::unique_ptr<WebSocketServerCallbacks> callbacks_;
  std::unique_ptr<foxglove_websocket_server, foxglove_error (*)(foxglove_websocket_server*)> impl_;
};

}  // namespace foxglove
