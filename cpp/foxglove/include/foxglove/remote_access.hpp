#pragma once

#include <foxglove/channel.hpp>
#include <foxglove/connection_graph.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/fetch_asset.hpp>
#include <foxglove/parameter.hpp>
#include <foxglove/service.hpp>

#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <vector>

enum foxglove_error : uint8_t;
struct foxglove_gateway;

namespace foxglove {

/// @brief Connection status of the remote access gateway.
enum class RemoteAccessConnectionStatus : uint8_t {
  /// The gateway is attempting to establish or re-establish a connection.
  Connecting = 0,
  /// The gateway is connected and handling events.
  Connected = 1,
  /// The gateway is shutting down. Listener callbacks may still be in progress.
  ShuttingDown = 2,
  /// The gateway has been shut down. No further listener callbacks will be invoked.
  Shutdown = 3,
};

/// @brief Level indicator for a remote access gateway status message.
enum class RemoteAccessStatusLevel : uint8_t {
  /// Info level.
  Info = 0,
  /// Warning level.
  Warning = 1,
  /// Error level.
  Error = 2,
};

/// @brief Capabilities that a remote access gateway may advertise to clients.
enum class RemoteAccessGatewayCapabilities : uint8_t {
  /// No capabilities.
  None = 0,
  /// Allow clients to advertise channels to send data messages to the server.
  ClientPublish = 1 << 0,
  /// Allow clients to get, set, and subscribe to parameter updates.
  Parameters = 1 << 1,
  /// Allow clients to call services.
  Services = 1 << 2,
  /// Allow clients to subscribe and make connection graph updates.
  ConnectionGraph = 1 << 3,
  /// Allow clients to request assets.
  Assets = 1 << 4,
};

/// @brief Combine two gateway capabilities.
inline RemoteAccessGatewayCapabilities operator|(
  RemoteAccessGatewayCapabilities a, RemoteAccessGatewayCapabilities b
) {
  return RemoteAccessGatewayCapabilities(uint8_t(a) | uint8_t(b));
}

/// @brief Check if a gateway capability is set.
inline RemoteAccessGatewayCapabilities operator&(
  RemoteAccessGatewayCapabilities a, RemoteAccessGatewayCapabilities b
) {
  return RemoteAccessGatewayCapabilities(uint8_t(a) & uint8_t(b));
}

/// @brief Callback interface for the remote access gateway.
///
/// These methods are invoked from time-sensitive contexts and must not block.
///
/// @note These callbacks may be invoked concurrently from multiple threads.
/// You must synchronize access to your mutable internal state or shared resources.
struct RemoteAccessGatewayCallbacks {
  /// @brief Callback invoked when the gateway connection status changes.
  std::function<void(RemoteAccessConnectionStatus status)> onConnectionStatusChanged;

  /// @brief Callback invoked when a remote client subscribes to a channel.
  std::function<void(uint32_t client_id, const ChannelDescriptor& channel)> onSubscribe;

  /// @brief Callback invoked when a remote client unsubscribes from a channel or disconnects.
  /// Also invoked when a subscribed channel is removed from the Context.
  std::function<void(uint32_t client_id, const ChannelDescriptor& channel)> onUnsubscribe;

  /// @brief Callback invoked when a client message is received.
  std::function<void(
    uint32_t client_id, const ChannelDescriptor& channel, const std::byte* data, size_t data_len
  )>
    onMessageData;

  /// @brief Callback invoked when a client advertises a channel.
  std::function<void(uint32_t client_id, const ChannelDescriptor& channel)> onClientAdvertise;

  /// @brief Callback invoked when a client unadvertises a channel.
  std::function<void(uint32_t client_id, const ChannelDescriptor& channel)> onClientUnadvertise;

  /// @brief Callback invoked when a client requests parameters.
  ///
  /// Requires RemoteAccessGatewayCapabilities::Parameters.
  std::function<std::vector<Parameter>(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<std::string_view>& param_names
  )>
    onGetParameters;

  /// @brief Callback invoked when a client sets parameters.
  ///
  /// Requires RemoteAccessGatewayCapabilities::Parameters.
  std::function<std::vector<Parameter>(
    uint32_t client_id, std::optional<std::string_view> request_id,
    const std::vector<ParameterView>& params
  )>
    onSetParameters;

  /// @brief Callback invoked when a client subscribes to parameters for the first time.
  ///
  /// Requires RemoteAccessGatewayCapabilities::Parameters.
  std::function<void(const std::vector<std::string_view>& param_names)> onParametersSubscribe;

  /// @brief Callback invoked when the last client unsubscribes from parameters.
  ///
  /// Requires RemoteAccessGatewayCapabilities::Parameters.
  std::function<void(const std::vector<std::string_view>& param_names)> onParametersUnsubscribe;

  /// @brief Callback invoked when the first client subscribes to connection graph updates.
  ///
  /// Requires RemoteAccessGatewayCapabilities::ConnectionGraph.
  ///
  /// @warning Do not call publishConnectionGraph from within this callback; doing so will deadlock.
  std::function<void()> onConnectionGraphSubscribe;

  /// @brief Callback invoked when the last client unsubscribes from connection graph updates.
  ///
  /// Requires RemoteAccessGatewayCapabilities::ConnectionGraph.
  ///
  /// @warning Do not call publishConnectionGraph from within this callback; doing so will deadlock.
  std::function<void()> onConnectionGraphUnsubscribe;
};

/// @brief Options for creating a remote access gateway.
struct RemoteAccessGatewayOptions {
  /// @brief The logging context for this gateway.
  Context context;
  /// @brief The name of the device/server reported in the ServerInfo message.
  std::string name;
  /// @brief Device token for Foxglove platform authentication.
  ///
  /// If empty, the token is read from the `FOXGLOVE_DEVICE_TOKEN` environment variable.
  std::string device_token;
  /// @brief Event callbacks.
  RemoteAccessGatewayCallbacks callbacks;
  /// @brief Advertised capabilities.
  RemoteAccessGatewayCapabilities capabilities = RemoteAccessGatewayCapabilities::None;
  /// @brief Supported encodings for client requests.
  std::vector<std::string> supported_encodings;
  /// @brief A fetch asset handler callback.
  FetchAssetHandler fetch_asset;
  /// @brief A sink channel filter callback.
  SinkChannelFilterFn sink_channel_filter;
  /// @brief Override the Foxglove API base URL.
  std::optional<std::string> foxglove_api_url;
  /// @brief Override the Foxglove API timeout (in seconds).
  std::optional<uint64_t> foxglove_api_timeout_secs;
  /// @brief Override the message backlog size.
  std::optional<size_t> message_backlog_size;
};

/// @brief A remote access gateway for live visualization and teleop in Foxglove.
///
/// The gateway connects to the Foxglove platform and allows remote clients to
/// subscribe to channels and receive data.
///
/// @note RemoteAccessGateway is fully thread-safe, but RemoteAccessGatewayCallbacks may be invoked
/// concurrently from multiple threads, so you will need to use synchronization in your callbacks.
class RemoteAccessGateway final {
public:
  /// @brief Create and start a gateway with the given options.
  static FoxgloveResult<RemoteAccessGateway> create(RemoteAccessGatewayOptions&& options);

  /// @brief Get the current connection status.
  [[nodiscard]] RemoteAccessConnectionStatus connectionStatus() const;

  /// @brief Advertises support for the provided service.
  ///
  /// @param service The service to add.
  [[nodiscard]] FoxgloveError addService(Service&& service) const noexcept;

  /// @brief Removes a service that was previously advertised.
  ///
  /// @param name The name of the service to remove.
  [[nodiscard]] FoxgloveError removeService(std::string_view name) const noexcept;

  /// @brief Publishes parameter values to all subscribed clients.
  ///
  /// Requires RemoteAccessGatewayCapabilities::Parameters.
  ///
  /// @param params Updated parameters.
  void publishParameterValues(std::vector<Parameter>&& params);

  /// @brief Publishes a status message to all connected participants.
  ///
  /// The caller may optionally provide a message ID, which can be used in a
  /// subsequent call to `removeStatus()`.
  ///
  /// @param level Status level value.
  /// @param message Status message.
  /// @param id Optional message ID.
  [[nodiscard]] FoxgloveError publishStatus(
    RemoteAccessStatusLevel level, std::string_view message,
    std::optional<std::string_view> id = std::nullopt
  ) const noexcept;

  /// @brief Removes status messages from all connected participants.
  ///
  /// Previously published status messages are referenced by ID.
  ///
  /// @param ids Message IDs.
  [[nodiscard]] FoxgloveError removeStatus(const std::vector<std::string_view>& ids) const;

  /// @brief Publish a connection graph to all subscribed clients.
  ///
  /// Requires RemoteAccessGatewayCapabilities::ConnectionGraph.
  ///
  /// @param graph The connection graph to publish.
  [[nodiscard]] FoxgloveError publishConnectionGraph(const ConnectionGraph& graph) const;

  /// @brief Gracefully shut down the gateway.
  FoxgloveError stop();

private:
  RemoteAccessGateway(
    foxglove_gateway* gateway, std::unique_ptr<RemoteAccessGatewayCallbacks> callbacks,
    std::unique_ptr<FetchAssetHandler> fetch_asset,
    std::unique_ptr<SinkChannelFilterFn> sink_channel_filter
  );

  std::unique_ptr<RemoteAccessGatewayCallbacks> callbacks_;
  std::unique_ptr<FetchAssetHandler> fetch_asset_;
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter_;
  std::unique_ptr<foxglove_gateway, foxglove_error (*)(foxglove_gateway*)> impl_;
};

}  // namespace foxglove
