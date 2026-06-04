#pragma once

#include <chrono>
#include <condition_variable>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <queue>
#include <string>
#include <thread>
#include <unordered_map>
#include <variant>
#include <vector>

#include <foxglove/fetch_asset.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/parameter_handler.hpp>
#include <foxglove/system_info.hpp>
#include <foxglove/websocket.hpp>
#ifdef FOXGLOVE_REMOTE_ACCESS
#include <foxglove/remote_access.hpp>
#endif

#include <foxglove_bridge_core/logging.hpp>
#include <foxglove_bridge_core/types.hpp>

namespace foxglove_bridge {

using ParameterList = std::vector<foxglove::Parameter>;

/// Deep-copy a parameter list (foxglove::Parameter is move-only).
ParameterList cloneParameterList(const ParameterList& params);

/// Implemented by the ROS frontend. Receives callbacks from both transports
/// (WebSocket server and remote access gateway), normalized into a single
/// interface; `isGateway` distinguishes the transport where the frontend needs
/// to keep state separate (e.g. client IDs are only unique per transport).
///
/// Exception behavior mirrors the underlying SDK transports: exceptions thrown
/// from the WebSocket-side callbacks propagate to the SDK (which reports them
/// to the client), while exceptions thrown from gateway-side callbacks are
/// caught and logged by the TransportManager.
class BridgeDelegate {
public:
  virtual ~BridgeDelegate() = default;

  virtual void onSubscribe(ChannelId channelId, ClientId clientId, bool isGateway,
                           std::optional<SinkId> sinkId) = 0;
  virtual void onUnsubscribe(ChannelId channelId, ClientId clientId, bool isGateway) = 0;

  // Client publish. Only called if the ClientPublish capability is enabled.
  virtual void onClientAdvertise(const ClientChannelInfo& channel, ClientId clientId,
                                 bool isGateway) = 0;
  virtual void onClientUnadvertise(ChannelId clientChannelId, ClientId clientId,
                                   bool isGateway) = 0;
  virtual void onClientMessage(ChannelId clientChannelId, ClientId clientId, bool isGateway,
                               const std::byte* data, size_t dataLen) = 0;

  // Connection graph subscription tracking (refcounted by the frontend).
  virtual void onConnectionGraphSubscribe(bool subscribe) = 0;

  // Only called if the Assets capability is enabled.
  virtual void fetchAsset(std::string_view uri, foxglove::FetchAssetResponder&& responder) = 0;

  virtual void onClientConnect() {}
  virtual void onClientDisconnect() {}

#ifdef FOXGLOVE_REMOTE_ACCESS
  /// QoS classification for remote access channels (lossy data track vs
  /// reliable control channel).
  virtual foxglove::QosProfile classifyRemoteAccessQos(const foxglove::ChannelDescriptor& channel) {
    (void)channel;
    return foxglove::QosProfile{};
  }
  virtual void onGatewayConnectionStatusChanged(foxglove::RemoteAccessConnectionStatus status) {
    (void)status;
  }
#endif
};

/// Implemented by the ROS frontend to back parameter get/set/subscribe
/// requests (e.g. via rclcpp parameter clients on ROS 2, or the master
/// parameter server API on ROS 1). All calls are made from the
/// TransportManager's parameter worker thread.
class ParameterBackend {
public:
  virtual ~ParameterBackend() = default;

  virtual ParameterList getParams(const std::vector<std::string_view>& paramNames,
                                  const std::chrono::duration<double>& timeout) = 0;
  virtual void setParams(const ParameterList& params,
                         const std::chrono::duration<double>& timeout) = 0;
  virtual void subscribeParams(const std::vector<std::string_view>& paramNames) = 0;
  virtual void unsubscribeParams(const std::vector<std::string_view>& paramNames) = 0;
};

struct TransportOptions {
  std::string host = "0.0.0.0";
  uint16_t port = 8765;
  std::vector<std::string> supportedEncodings;
  /// Capability names, as in the `capabilities` bridge parameter.
  std::vector<std::string> capabilities;
  /// Adds the Time capability to the WebSocket server (for use_sim_time).
  bool broadcastTimeCapability = false;
  std::optional<std::map<std::string, std::string>> serverInfo;
  size_t messageBacklogSize = 1024;

  bool useTls = false;
  std::string certfile;
  std::string keyfile;

  /// Wire onClientConnect/onClientDisconnect delegate callbacks.
  bool notifyClientCount = false;

  /// Remote access gateway. Requires a build with FOXGLOVE_REMOTE_ACCESS;
  /// enabling it otherwise throws from the TransportManager constructor.
  bool remoteAccess = false;
  /// Falls back to the FOXGLOVE_DEVICE_TOKEN environment variable when empty.
  std::string deviceToken;
  /// Empty string means the SDK default.
  std::string foxgloveApiUrl;

  bool sysinfo = false;
  std::string sysinfoTopic;
  std::chrono::milliseconds sysinfoRefreshInterval{500};
};

/// Owns the SDK transports (WebSocket server + optional remote access
/// gateway) plus the SDK context, the system info publisher, and the
/// parameter op queue. Normalizes the two transports' callbacks into the
/// single BridgeDelegate interface and fans bridge-side publishes
/// (services, connection graph, parameter values) out to both transports.
class TransportManager {
public:
  /// May throw std::runtime_error / std::invalid_argument on invalid options
  /// or transport startup failure. The delegate and paramBackend (if not
  /// null) must outlive the TransportManager. paramBackend may be null when
  /// the Parameters capability is not requested.
  TransportManager(TransportOptions options, BridgeDelegate& delegate,
                   ParameterBackend* paramBackend, Logger logger);
  ~TransportManager();

  TransportManager(const TransportManager&) = delete;
  TransportManager& operator=(const TransportManager&) = delete;

  /// Stop the transports and the parameter worker. Called by the destructor;
  /// call explicitly when shutdown ordering relative to frontend state
  /// matters. Idempotent.
  void stop();

  const foxglove::Context& context() const {
    return _context;
  }
  foxglove::WebSocketServerCapabilities capabilities() const {
    return _capabilities;
  }
  bool hasCapability(foxglove::WebSocketServerCapabilities capability) const;

  uint16_t port() const;
  size_t clientCount() const;
  void broadcastTime(uint64_t timestampNs);

  bool hasGateway() const;

  /// Add/remove a service on both transports. The handler must remain valid
  /// until the service is removed (the frontend owns handler storage).
  /// Returns false if the service could not be added to the WebSocket server;
  /// gateway-side failures are logged but do not fail the call.
  /// (Non-const refs because foxglove::Service::create takes non-const refs.)
  bool addService(const std::string& name, foxglove::ServiceSchema& schema,
                  foxglove::ServiceHandler& handler);
  void removeService(const std::string& name);

  /// Publish the connection graph to both transports.
  void publishConnectionGraph(foxglove::ConnectionGraph& graph);

  /// Publish updated parameter values to both transports.
  void publishParameterValues(const ParameterList& parameters);

private:
  // Each parameter op carries enough state to be handled by the worker thread.
  // Get and Set ops own their responders; Subscribe/Unsubscribe carry just the
  // parameter names so they serialize with get/set on the same queue.
  struct GetParamsOp {
    std::vector<std::string> names;
    foxglove::GetParametersResponder responder;
  };
  struct SetParamsOp {
    ParameterList parameters;
    foxglove::SetParametersResponder responder;
  };
  struct SubscribeParamsOp {
    std::vector<std::string> names;
  };
  struct UnsubscribeParamsOp {
    std::vector<std::string> names;
  };
  using ParameterOp =
    std::variant<GetParamsOp, SetParamsOp, SubscribeParamsOp, UnsubscribeParamsOp>;

  void wireWebSocketCallbacks(foxglove::WebSocketServerOptions& serverOptions);
#ifdef FOXGLOVE_REMOTE_ACCESS
  void createGateway(const TransportOptions& options,
                     const std::optional<std::map<std::string, std::string>>& serverInfo);
#endif

  // Wire the parameter-related callbacks/handler on a server or gateway
  // options struct. Both options structs share these field types; this helper
  // keeps the WS and gateway sites in sync.
  void wireParameterCallbacks(
    std::function<void(const std::vector<std::string_view>&)>& onSubscribe,
    std::function<void(const std::vector<std::string_view>&)>& onUnsubscribe,
    foxglove::ParameterHandler& handler);

  void enqueueParameterOp(ParameterOp&& op);
  void parameterWorkerLoop();
  void handleGetParams(GetParamsOp&& op);
  void handleSetParams(SetParamsOp&& op);
  void handleSubscribeParams(SubscribeParamsOp&& op);
  void handleUnsubscribeParams(UnsubscribeParamsOp&& op);

  Logger _log;
  BridgeDelegate& _delegate;
  ParameterBackend* _paramBackend = nullptr;

  foxglove::Context _context;
  foxglove::WebSocketServerCapabilities _capabilities =
    foxglove::WebSocketServerCapabilities::None;
  std::unique_ptr<foxglove::WebSocketServer> _server;
  std::unique_ptr<foxglove::SystemInfoPublisher> _sysinfoPublisher;
#ifdef FOXGLOVE_REMOTE_ACCESS
  std::unique_ptr<foxglove::RemoteAccessGateway> _gateway;
#endif

  std::mutex _paramOpMutex;
  std::condition_variable _paramOpCv;
  std::queue<ParameterOp> _paramOpQueue;
  bool _paramOpShutdown = false;
  std::unique_ptr<std::thread> _paramWorkerThread;

  // Parameter subscription refcount, owned by the worker thread.
  //
  // The websocket server and remote access gateway each independently maintain
  // state about parameter subscriptions on behalf of clients. They each fire
  // onParametersSubscribe when the first subscriber subscribes to a particular
  // parameter, and onParametersUnsubscribe when the last subscriber
  // unsubscribes. We use this map to aggregate subscriptions across the two
  // transports.
  std::unordered_map<std::string, int> _paramSubscriberCount;

  bool _stopped = false;
};

}  // namespace foxglove_bridge
