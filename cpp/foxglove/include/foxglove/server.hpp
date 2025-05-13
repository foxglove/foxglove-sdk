#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server/connection_graph.hpp>
#include <foxglove/server/fetch_asset.hpp>
#include <foxglove/server/parameter.hpp>
#include <foxglove/server/service.hpp>

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
  /// Allow clients to request assets. If you supply an asset handler to the
  /// server, this capability will be advertised automatically.
  Assets = 1 << 5,
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
  /// Requires the capability WebSocketServerCapabilities::Parameters.
  ///
  /// @param client_id The client ID.
  /// @param request_id A request ID unique to this client. May be NULL.
  /// @param param_names A list of parameter names to fetch. If empty, this
  /// method should return all parameters.
  std::function<std::vector<Parameter>(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<std::string_view>& param_names
  )>
    onGetParameters;

  /// @brief Callback invoked when a client sets parameters.
  ///
  /// Requires the capability WebSocketServerCapabilities::Parameters.
  ///
  /// This function should return the updated parameters. All clients subscribed
  /// to updates for the returned parameters will be notified.
  ///
  /// @param client_id The client ID.
  /// @param request_id A request ID unique to this client. May be NULL.
  /// @param param_names A list of updated parameter values.
  std::function<std::vector<Parameter>(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<ParameterView>& params
  )>
    onSetParameters;

  /// @brief Callback invoked when a client subscribes to the named parameters
  /// for the first time.
  ///
  /// Requires the capability WebSocketServerCapabilities::Parameters.
  ///
  /// @param param_names A list of parameter names.
  std::function<void(const std::vector<std::string_view>& param_names)> onParametersSubscribe;

  /// @brief Callback invoked when the last client unsubscribes from the named
  /// parameters.
  ///
  /// Requires the capability WebSocketServerCapabilities::Parameters.
  ///
  /// @param param_names A list of parameter names.
  std::function<void(const std::vector<std::string_view>& param_names)> onParametersUnsubscribe;

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
  /// @brief A fetch asset handler callback.
  FetchAssetHandler fetch_asset;
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

  /// @brief Publishes the current server timestamp to all clients.
  ///
  /// Requires the capability WebSocketServerCapabilities::Time.
  ///
  /// @param timestamp_nanos An epoch offset in nanoseconds.
  void broadcastTime(uint64_t timestamp_nanos) const noexcept;

  /// @brief Advertises support for the provided service.
  ///
  /// This service will be available for clients to use until it is removed with
  /// `removeService()`, or the server is stopped.
  ///
  /// This method will fail for various reasons, with the following error codes:
  ///
  /// - `DuplicateService`: A service with the same name is already registered.
  /// - `MissingRequestedEncoding`: The service didn't declare a request
  ///   encoding, and the server was not configured with a global list of
  ///   supported encodings.
  /// - `ServicesNotSupported`: The server was not convfigured with the
  ///   `Services` capability.
  ///
  /// @param service The service to add.
  [[nodiscard]] FoxgloveError addService(Service&& service) const noexcept;

  /// @brief Removes a service that was previously advertised.
  ///
  /// This method will fail with `FoxgloveError::Utf8Error` if the service name
  /// is not a valid UTF-8 string.
  ///
  /// @param name The name of the service to remove.
  [[nodiscard]] FoxgloveError removeService(std::string_view name) const noexcept;

  /// @brief Publishes parameter values to all subscribed clients.
  ///
  /// Requires the capability WebSocketServerCapabilities::Parameters.
  ///
  /// @param params Updated parameters.
  void publishParameterValues(std::vector<Parameter>&& params);

  /// @brief Publish a connection graph to all subscribed clients.
  ///
  /// @param graph The connection graph to publish.
  ///
  /// This requires the capability WebSocketServerCapabilities::ConnectionGraph
  void publishConnectionGraph(ConnectionGraph& graph);

private:
  WebSocketServer(
    foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks,
    std::unique_ptr<FetchAssetHandler> fetch_asset
  );

  std::unique_ptr<WebSocketServerCallbacks> callbacks_;
  std::unique_ptr<FetchAssetHandler> fetch_asset_;
  std::unique_ptr<foxglove_websocket_server, foxglove_error (*)(foxglove_websocket_server*)> impl_;
};

}  // namespace foxglove
