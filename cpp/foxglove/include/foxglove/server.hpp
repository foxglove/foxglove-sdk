#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server/connection_graph.hpp>

#include <cstdint>
#include <functional>
#include <memory>
#include <string>

enum foxglove_error : uint8_t;
struct foxglove_websocket_server;
struct foxglove_connection_graph;

namespace foxglove {

/// @brief A channel advertised by a client.
struct ClientChannel {
  /// @brief The ID of the channel.
  uint32_t id;
  /// @brief The topic of the channel.
  std::string_view topic;
  /// @brief The encoding of the channel.
  std::string_view encoding;
  /// @brief The name of the schema of the channel.
  std::string_view schema_name;
  /// @brief The encoding of the schema of the channel.
  std::string_view schema_encoding;
  /// @brief The schema of the channel.
  const std::byte* schema;
  /// @brief The length of the schema of the channel.
  size_t schema_len;
};

/// @brief The capabilities of a WebSocket server.
///
/// A server may advertise certain capabilities to clients and provide related functionality
/// in WebSocketServerCallbacks.
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

/// @brief Combine two capabilities.
inline WebSocketServerCapabilities operator|(
  WebSocketServerCapabilities a, WebSocketServerCapabilities b
) {
  return WebSocketServerCapabilities(uint8_t(a) | uint8_t(b));
}

/// @brief Check if a capability is set.
inline WebSocketServerCapabilities operator&(
  WebSocketServerCapabilities a, WebSocketServerCapabilities b
) {
  return WebSocketServerCapabilities(uint8_t(a) & uint8_t(b));
}

/// @brief The callback interface for a WebSocket server.
///
/// These methods are invoked from the client's main poll loop and must be as low-latency as
/// possible.
struct WebSocketServerCallbacks {
  /// @brief Callback invoked when a client subscribes to a channel.
  ///
  /// Only invoked if the channel is associated with the server and isn't already subscribed to by
  /// the client.
  std::function<void(uint64_t channel_id)> onSubscribe;
  /// @brief Callback invoked when a client unsubscribes from a channel.
  ///
  /// Only invoked for channels that had an active subscription from the client.
  std::function<void(uint64_t channel_id)> onUnsubscribe;
  /// @brief Callback invoked when a client advertises a client channel.
  ///
  /// Requires the capability WebSocketServerCapabilities::ClientPublish
  std::function<void(uint32_t client_id, const ClientChannel& channel)> onClientAdvertise;
  /// @brief Callback invoked when a client message is received
  std::function<
    void(uint32_t client_id, uint32_t client_channel_id, const std::byte* data, size_t data_len)>
    onMessageData;
  /// @brief Callback invoked when a client unadvertises a client channel.
  ///
  /// Requires the capability WebSocketServerCapabilities::ClientPublish
  std::function<void(uint32_t client_id, uint32_t client_channel_id)> onClientUnadvertise;
  /// @brief Callback invoked when a client requests parameters.
  ///
  /// Requires the capability WebSocketServerCapabilities::Parameters
  std::function<void()> onGetParameters;
  /// @brief Callback invoked when a client requests connection graph updates.
  ///
  /// Requires the capability WebSocketServerCapabilities::ConnectionGraph
  std::function<void()> onConnectionGraphSubscribe;
  /// @brief Callback invoked when a client unsubscribes from connection graph updates.
  ///
  /// Requires the capability WebSocketServerCapabilities::ConnectionGraph
  std::function<void()> onConnectionGraphUnsubscribe;
};

/// @brief Options for a WebSocket server.
struct WebSocketServerOptions {
  friend class WebSocketServer;

  /// @brief The logging context for this server.
  Context context;
  /// @brief The name of the server.
  std::string name;
  /// @brief The host address of the server.
  std::string host = "127.0.0.1";
  /// @brief The port of the server. Default is 8765, which matches the Foxglove app.
  uint16_t port = 8765;
  /// @brief The callbacks of the server.
  WebSocketServerCallbacks callbacks;
  /// @brief The capabilities of the server.
  WebSocketServerCapabilities capabilities = WebSocketServerCapabilities(0);
  /// @brief The supported encodings of the server.
  std::vector<std::string> supported_encodings;
};

/// @brief A WebSocket server for visualization in Foxglove.
///
/// After your server is started, you can open the Foxglove app to visualize your data. See
/// [Connecting to data].
///
/// [Connecting to data]: https://docs.foxglove.dev/docs/connecting-to-data/introduction
class WebSocketServer final {
public:
  /// @brief Create a new WebSocket server with the given options.
  static FoxgloveResult<WebSocketServer> create(WebSocketServerOptions&& options);

  /// Get the port on which the server is listening.
  [[nodiscard]] uint16_t port() const;

  /// @brief Gracefully shut down the websocket server.
  FoxgloveError stop();

  /// @brief Publish a connection graph to all subscribed clients.
  ///
  /// @param graph The connection graph to publish.
  ///
  /// This requires the capability WebSocketServerCapabilities::ConnectionGraph
  void publishConnectionGraph(ConnectionGraph& graph);

private:
  WebSocketServer(
    foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks
  );

  std::unique_ptr<WebSocketServerCallbacks> callbacks_;
  std::unique_ptr<foxglove_websocket_server, foxglove_error (*)(foxglove_websocket_server*)> impl_;
};

}  // namespace foxglove
