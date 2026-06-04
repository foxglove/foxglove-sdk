#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <type_traits>

#include <foxglove_bridge_core/capabilities.hpp>
#include <foxglove_bridge_core/transport_manager.hpp>

namespace foxglove_bridge {

namespace {

std::vector<std::byte> readFile(const std::string& filepath) {
  std::ifstream file(filepath, std::ios::binary | std::ios::ate);
  if (!file.is_open()) {
    throw std::runtime_error("Failed to open file: " + filepath);
  }

  std::streamsize length = file.tellg();
  file.seekg(0, std::ios::beg);

  std::vector<std::byte> buffer(length);
  if (!file.read(reinterpret_cast<char*>(buffer.data()), length)) {
    throw std::runtime_error("Failed to read file: " + filepath);
  }

  return buffer;
}

#ifdef FOXGLOVE_REMOTE_ACCESS
ClientChannelInfo toClientChannelInfo(const foxglove::ChannelDescriptor& channel) {
  ClientChannelInfo info;
  info.id = channel.id();
  info.topic = std::string(channel.topic());
  info.encoding = std::string(channel.messageEncoding());
  auto schema = channel.schema();
  if (schema.has_value()) {
    info.schemaName = schema->name;
    info.schemaData = schema->data;
    info.schemaLen = schema->data_len;
  }
  return info;
}
#endif

ClientChannelInfo toClientChannelInfo(const foxglove::ClientChannel& channel) {
  ClientChannelInfo info;
  info.id = channel.id;
  info.topic = std::string(channel.topic);
  info.encoding = std::string(channel.encoding);
  info.schemaName = std::string(channel.schema_name);
  info.schemaData = channel.schema;
  info.schemaLen = channel.schema_len;
  return info;
}

}  // namespace

ParameterList cloneParameterList(const ParameterList& params) {
  ParameterList cloned;
  cloned.reserve(params.size());
  for (const auto& param : params) {
    cloned.push_back(param.clone());
  }
  return cloned;
}

TransportManager::TransportManager(TransportOptions options, BridgeDelegate& delegate,
                                   ParameterBackend* paramBackend, Logger logger)
    : _log(std::move(logger))
    , _delegate(delegate)
    , _paramBackend(paramBackend)
    , _context(foxglove::Context::create()) {
  _capabilities = processCapabilities(options.capabilities);

  foxglove::WebSocketServerOptions serverOptions;
  serverOptions.host = options.host;
  serverOptions.port = options.port;
  serverOptions.supported_encodings = options.supportedEncodings;
  serverOptions.capabilities = _capabilities;
  serverOptions.context = _context;
  serverOptions.server_info = options.serverInfo;
  serverOptions.message_backlog_size = options.messageBacklogSize;

  if (options.broadcastTimeCapability) {
    serverOptions.capabilities =
      serverOptions.capabilities | foxglove::WebSocketServerCapabilities::Time;
  }

  // If TLS is enabled, load the certificate and key files from disk
  if (options.useTls) {
    if (options.certfile.empty() || !std::filesystem::exists(options.certfile)) {
      throw std::invalid_argument("certfile must be provided when TLS is enabled and must exist");
    }

    if (options.keyfile.empty() || !std::filesystem::exists(options.keyfile)) {
      throw std::invalid_argument("keyfile must be provided when TLS is enabled and must exist");
    }

    serverOptions.tls_identity = foxglove::TlsIdentity{};
    serverOptions.tls_identity->cert = readFile(options.certfile);
    serverOptions.tls_identity->key = readFile(options.keyfile);
  }

  if (options.notifyClientCount) {
    serverOptions.callbacks.onClientConnect = [this]() {
      _delegate.onClientConnect();
    };
    serverOptions.callbacks.onClientDisconnect = [this]() {
      _delegate.onClientDisconnect();
    };
  }

  wireWebSocketCallbacks(serverOptions);

  auto maybeSdkServer = foxglove::WebSocketServer::create(std::move(serverOptions));
  if (!maybeSdkServer.has_value()) {
    throw std::runtime_error(std::string("Couldn't initialize websocket server: ") +
                             foxglove::strerror(maybeSdkServer.error()));
  }

  // Constructing an SDK server also starts it listening automatically
  _server = std::make_unique<foxglove::WebSocketServer>(std::move(maybeSdkServer.value()));

  if (options.sysinfo) {
    foxglove::SystemInfoOptions sysinfoOptions;
    sysinfoOptions.context = _context;
    sysinfoOptions.topic = options.sysinfoTopic;
    sysinfoOptions.refresh_interval = options.sysinfoRefreshInterval;
    auto maybeSysinfo = foxglove::SystemInfoPublisher::create(std::move(sysinfoOptions));
    if (!maybeSysinfo.has_value()) {
      _log.log(BridgeLogLevel::Warn, "Couldn't start system info publisher: %s",
               foxglove::strerror(maybeSysinfo.error()));
    } else {
      _sysinfoPublisher =
        std::make_unique<foxglove::SystemInfoPublisher>(std::move(maybeSysinfo.value()));
    }
  }

#ifndef FOXGLOVE_REMOTE_ACCESS
  if (options.remoteAccess) {
    throw std::runtime_error(
      "remote_access is set to true but the bridge was not built with "
      "FOXGLOVE_BRIDGE_REMOTE_ACCESS=ON. Remote access is not available.");
  }
#else
  if (options.remoteAccess) {
    createGateway(options, options.serverInfo);
  }
#endif

  if (_paramBackend != nullptr &&
      hasCapability(foxglove::WebSocketServerCapabilities::Parameters)) {
    _paramWorkerThread = std::make_unique<std::thread>([this]() {
      parameterWorkerLoop();
    });
  }
}

TransportManager::~TransportManager() {
  stop();
}

void TransportManager::wireWebSocketCallbacks(foxglove::WebSocketServerOptions& serverOptions) {
  // Exceptions from these callbacks propagate to the SDK, mirroring the
  // behavior of the pre-extraction bridge.
  serverOptions.callbacks.onConnectionGraphSubscribe = [this]() {
    _delegate.onConnectionGraphSubscribe(true);
  };
  serverOptions.callbacks.onConnectionGraphUnsubscribe = [this]() {
    _delegate.onConnectionGraphSubscribe(false);
  };
  serverOptions.callbacks.onSubscribe = [this](ChannelId channelId,
                                               const foxglove::ClientMetadata& client) {
    _delegate.onSubscribe(channelId, client.id, false, client.sink_id);
  };
  serverOptions.callbacks.onUnsubscribe = [this](ChannelId channelId,
                                                 const foxglove::ClientMetadata& client) {
    _delegate.onUnsubscribe(channelId, client.id, false);
  };

  if (hasCapability(foxglove::WebSocketServerCapabilities::ClientPublish)) {
    serverOptions.callbacks.onClientAdvertise = [this](ClientId clientId,
                                                       const foxglove::ClientChannel& channel) {
      _delegate.onClientAdvertise(toClientChannelInfo(channel), clientId, false);
    };
    serverOptions.callbacks.onClientUnadvertise = [this](ClientId clientId,
                                                         ChannelId clientChannelId) {
      _delegate.onClientUnadvertise(clientChannelId, clientId, false);
    };
    serverOptions.callbacks.onMessageData = [this](ClientId clientId, ChannelId clientChannelId,
                                                   const std::byte* data, size_t dataLen) {
      _delegate.onClientMessage(clientChannelId, clientId, false, data, dataLen);
    };
  }

  if (hasCapability(foxglove::WebSocketServerCapabilities::Assets)) {
    serverOptions.fetch_asset = [this](std::string_view uri,
                                       foxglove::FetchAssetResponder&& responder) {
      _delegate.fetchAsset(uri, std::move(responder));
    };
  }

  if (_paramBackend != nullptr &&
      hasCapability(foxglove::WebSocketServerCapabilities::Parameters)) {
    wireParameterCallbacks(serverOptions.callbacks.onParametersSubscribe,
                           serverOptions.callbacks.onParametersUnsubscribe,
                           serverOptions.parameter_handler);
  }
}

#ifdef FOXGLOVE_REMOTE_ACCESS
void TransportManager::createGateway(
  const TransportOptions& options,
  const std::optional<std::map<std::string, std::string>>& serverInfo) {
  std::string deviceToken = options.deviceToken;
  if (deviceToken.empty()) {
    const char* envToken = std::getenv("FOXGLOVE_DEVICE_TOKEN");
    if (envToken != nullptr) {
      deviceToken = envToken;
    }
  }
  if (deviceToken.empty()) {
    _log.log(BridgeLogLevel::Fatal,
             "remote_access is enabled but no device_token was provided. "
             "Set FOXGLOVE_DEVICE_TOKEN or pass the device_token parameter.");
    throw std::runtime_error("missing device_token for remote_access");
  }

  foxglove::RemoteAccessGatewayOptions gatewayOptions;
  gatewayOptions.context = _context;
  gatewayOptions.device_token = deviceToken;
  gatewayOptions.supported_encodings = options.supportedEncodings;
  gatewayOptions.server_info = serverInfo;
  gatewayOptions.message_backlog_size = options.messageBacklogSize;

  if (!options.foxgloveApiUrl.empty()) {
    gatewayOptions.foxglove_api_url = options.foxgloveApiUrl;
  }

  gatewayOptions.capabilities = toGatewayCapabilities(_capabilities);

  // Exceptions from gateway-side delegate callbacks are caught and logged,
  // mirroring the behavior of the pre-extraction bridge.
  gatewayOptions.callbacks.onConnectionStatusChanged =
    [this](foxglove::RemoteAccessConnectionStatus status) {
      _delegate.onGatewayConnectionStatusChanged(status);
    };
  gatewayOptions.callbacks.onSubscribe = [this](uint32_t clientId,
                                                const foxglove::ChannelDescriptor& channel) {
    auto sinkId = _gateway->sinkId();
    if (!sinkId.has_value()) {
      _log.log(BridgeLogLevel::Warn,
               "Gateway: subscribe request for channel %lu (\"%s\") from client %u "
               "but gateway session has no sink ID (reconnecting?); "
               "cached transient_local messages will not be replayed",
               channel.id(), std::string(channel.topic()).c_str(), clientId);
    }
    _delegate.onSubscribe(channel.id(), clientId, true, sinkId);
  };
  gatewayOptions.callbacks.onUnsubscribe = [this](uint32_t clientId,
                                                  const foxglove::ChannelDescriptor& channel) {
    _delegate.onUnsubscribe(channel.id(), clientId, true);
  };
  gatewayOptions.qos_classifier = [this](const foxglove::ChannelDescriptor& channel) {
    return _delegate.classifyRemoteAccessQos(channel);
  };

  if (hasCapability(foxglove::WebSocketServerCapabilities::ClientPublish)) {
    gatewayOptions.callbacks.onClientAdvertise =
      [this](uint32_t clientId, const foxglove::ChannelDescriptor& channel) {
        try {
          _delegate.onClientAdvertise(toClientChannelInfo(channel), clientId, true);
        } catch (const std::exception& ex) {
          _log.log(BridgeLogLevel::Error, "Gateway: client advertise failed: %s", ex.what());
        }
      };
    gatewayOptions.callbacks.onClientUnadvertise =
      [this](uint32_t clientId, const foxglove::ChannelDescriptor& channel) {
        try {
          _delegate.onClientUnadvertise(channel.id(), clientId, true);
        } catch (const std::exception& ex) {
          _log.log(BridgeLogLevel::Error, "Gateway: client unadvertise failed: %s", ex.what());
        }
      };
    gatewayOptions.callbacks.onMessageData =
      [this](uint32_t clientId, const foxglove::ChannelDescriptor& channel, const std::byte* data,
             size_t dataLen) {
        try {
          _delegate.onClientMessage(channel.id(), clientId, true, data, dataLen);
        } catch (const std::exception& ex) {
          _log.log(BridgeLogLevel::Error, "Gateway: client message failed: %s", ex.what());
        }
      };
  }

  if (hasCapability(foxglove::WebSocketServerCapabilities::Assets)) {
    gatewayOptions.fetch_asset = [this](std::string_view uri,
                                        foxglove::FetchAssetResponder&& responder) {
      _delegate.fetchAsset(uri, std::move(responder));
    };
  }

  if (_paramBackend != nullptr &&
      hasCapability(foxglove::WebSocketServerCapabilities::Parameters)) {
    wireParameterCallbacks(gatewayOptions.callbacks.onParametersSubscribe,
                           gatewayOptions.callbacks.onParametersUnsubscribe,
                           gatewayOptions.parameter_handler);
  }

  if (hasCapability(foxglove::WebSocketServerCapabilities::ConnectionGraph)) {
    gatewayOptions.callbacks.onConnectionGraphSubscribe = [this]() {
      _delegate.onConnectionGraphSubscribe(true);
    };
    gatewayOptions.callbacks.onConnectionGraphUnsubscribe = [this]() {
      _delegate.onConnectionGraphSubscribe(false);
    };
  }

  auto maybeGateway = foxglove::RemoteAccessGateway::create(std::move(gatewayOptions));
  if (!maybeGateway.has_value()) {
    throw std::runtime_error(std::string("Failed to create remote access gateway: ") +
                             foxglove::strerror(maybeGateway.error()));
  }
  _gateway = std::make_unique<foxglove::RemoteAccessGateway>(std::move(maybeGateway.value()));
  _log.log(BridgeLogLevel::Info, "Remote access gateway started");
}
#endif

void TransportManager::stop() {
  if (_stopped) {
    return;
  }
  _stopped = true;

  if (_sysinfoPublisher) {
    _sysinfoPublisher->stop();
  }
#ifdef FOXGLOVE_REMOTE_ACCESS
  if (_gateway) {
    _gateway->stop();
  }
#endif
  if (_server) {
    _server->stop();
  }
  // Stop the parameter worker after the server and gateway are stopped, so no new ops
  // arrive while we're shutting it down. Any ops still in the queue get dropped.
  if (_paramWorkerThread) {
    std::queue<ParameterOp> drained;
    {
      std::lock_guard<std::mutex> lock(_paramOpMutex);
      _paramOpShutdown = true;
      std::swap(_paramOpQueue, drained);
    }
    _paramOpCv.notify_all();
    _paramWorkerThread->join();
    _paramWorkerThread.reset();
  }
}

bool TransportManager::hasCapability(foxglove::WebSocketServerCapabilities capability) const {
  return foxglove_bridge::hasCapability(_capabilities, capability);
}

uint16_t TransportManager::port() const {
  return _server->port();
}

size_t TransportManager::clientCount() const {
  return _server->clientCount();
}

void TransportManager::broadcastTime(uint64_t timestampNs) {
  _server->broadcastTime(timestampNs);
}

bool TransportManager::hasGateway() const {
#ifdef FOXGLOVE_REMOTE_ACCESS
  return _gateway != nullptr;
#else
  return false;
#endif
}

bool TransportManager::addService(const std::string& name, foxglove::ServiceSchema& schema,
                                  foxglove::ServiceHandler& handler) {
  auto serviceResult = foxglove::Service::create(name, schema, handler);
  if (!serviceResult.has_value()) {
    _log.log(BridgeLogLevel::Error, "Failed to create service %s: %s", name.c_str(),
             foxglove::strerror(serviceResult.error()));
    return false;
  }

  auto addServiceError = _server->addService(std::move(serviceResult.value()));
  if (addServiceError != foxglove::FoxgloveError::Ok) {
    _log.log(BridgeLogLevel::Error, "Failed to add service %s to server: %s", name.c_str(),
             foxglove::strerror(addServiceError));
    return false;
  }

#ifdef FOXGLOVE_REMOTE_ACCESS
  if (_gateway) {
    auto gatewayServiceResult = foxglove::Service::create(name, schema, handler);
    if (gatewayServiceResult.has_value()) {
      auto gatewayAddError = _gateway->addService(std::move(gatewayServiceResult.value()));
      if (gatewayAddError != foxglove::FoxgloveError::Ok) {
        _log.log(BridgeLogLevel::Error, "Failed to add service %s to gateway: %s", name.c_str(),
                 foxglove::strerror(gatewayAddError));
      }
    } else {
      _log.log(BridgeLogLevel::Error, "Failed to create gateway service %s: %s", name.c_str(),
               foxglove::strerror(gatewayServiceResult.error()));
    }
  }
#endif

  return true;
}

void TransportManager::removeService(const std::string& name) {
  auto error = _server->removeService(name);
  if (error != foxglove::FoxgloveError::Ok) {
    _log.log(BridgeLogLevel::Error, "Failed to remove service %s: %s", name.c_str(),
             foxglove::strerror(error));
  }
#ifdef FOXGLOVE_REMOTE_ACCESS
  if (_gateway) {
    auto gatewayError = _gateway->removeService(name);
    if (gatewayError != foxglove::FoxgloveError::Ok) {
      _log.log(BridgeLogLevel::Error, "Failed to remove service %s from gateway: %s", name.c_str(),
               foxglove::strerror(gatewayError));
    }
  }
#endif
}

void TransportManager::publishConnectionGraph(foxglove::ConnectionGraph& graph) {
  _server->publishConnectionGraph(graph);
#ifdef FOXGLOVE_REMOTE_ACCESS
  if (_gateway) {
    (void)_gateway->publishConnectionGraph(graph);
  }
#endif
}

void TransportManager::publishParameterValues(const ParameterList& parameters) {
  _server->publishParameterValues(cloneParameterList(parameters));
#ifdef FOXGLOVE_REMOTE_ACCESS
  if (_gateway) {
    _gateway->publishParameterValues(cloneParameterList(parameters));
  }
#endif
}

void TransportManager::wireParameterCallbacks(
  std::function<void(const std::vector<std::string_view>&)>& onSubscribe,
  std::function<void(const std::vector<std::string_view>&)>& onUnsubscribe,
  foxglove::ParameterHandler& handler) {
  onSubscribe = [this](const std::vector<std::string_view>& names) {
    SubscribeParamsOp op;
    op.names.reserve(names.size());
    for (const auto& name : names) {
      op.names.emplace_back(name);
    }
    enqueueParameterOp(std::move(op));
  };
  onUnsubscribe = [this](const std::vector<std::string_view>& names) {
    UnsubscribeParamsOp op;
    op.names.reserve(names.size());
    for (const auto& name : names) {
      op.names.emplace_back(name);
    }
    enqueueParameterOp(std::move(op));
  };
  handler.onGet = [this](uint32_t /*clientId*/, std::optional<std::string_view> /*requestId*/,
                         const std::vector<std::string_view>& names,
                         foxglove::GetParametersResponder&& responder) {
    GetParamsOp op{{}, std::move(responder)};
    op.names.reserve(names.size());
    for (const auto& name : names) {
      op.names.emplace_back(name);
    }
    enqueueParameterOp(std::move(op));
  };
  handler.onSet = [this](uint32_t /*clientId*/, std::optional<std::string_view> /*requestId*/,
                         const std::vector<foxglove::ParameterView>& params,
                         foxglove::SetParametersResponder&& responder) {
    SetParamsOp op{{}, std::move(responder)};
    op.parameters.reserve(params.size());
    for (const auto& param : params) {
      op.parameters.emplace_back(param.clone());
    }
    enqueueParameterOp(std::move(op));
  };
}

void TransportManager::enqueueParameterOp(ParameterOp&& op) {
  {
    std::lock_guard<std::mutex> lock(_paramOpMutex);
    if (_paramOpShutdown) {
      // Worker is gone; drop the op. Get/Set responders will send the requesting client an
      // error status on destruction; Subscribe/Unsubscribe are fire-and-forget.
      return;
    }
    _paramOpQueue.push(std::move(op));
  }
  _paramOpCv.notify_one();
}

void TransportManager::parameterWorkerLoop() {
  std::unique_lock<std::mutex> lock(_paramOpMutex);
  while (true) {
    _paramOpCv.wait(lock, [&] {
      return _paramOpShutdown || !_paramOpQueue.empty();
    });
    if (_paramOpShutdown && _paramOpQueue.empty()) {
      return;
    }
    auto op = std::move(_paramOpQueue.front());
    _paramOpQueue.pop();
    lock.unlock();

    std::visit(
      [this](auto& concrete) {
        using T = std::decay_t<decltype(concrete)>;
        if constexpr (std::is_same_v<T, GetParamsOp>) {
          this->handleGetParams(std::move(concrete));
        } else if constexpr (std::is_same_v<T, SetParamsOp>) {
          this->handleSetParams(std::move(concrete));
        } else if constexpr (std::is_same_v<T, SubscribeParamsOp>) {
          this->handleSubscribeParams(std::move(concrete));
        } else if constexpr (std::is_same_v<T, UnsubscribeParamsOp>) {
          this->handleUnsubscribeParams(std::move(concrete));
        }
      },
      op);

    lock.lock();
  }
}

void TransportManager::handleGetParams(GetParamsOp&& op) {
  std::vector<std::string_view> views(op.names.begin(), op.names.end());
  try {
    auto values = _paramBackend->getParams(views, std::chrono::seconds(5));
    std::move(op.responder).respond(std::move(values));
  } catch (const std::exception& ex) {
    _log.log(BridgeLogLevel::Error, "getParams failed: %s", ex.what());
    // Dropping the responder sends the client an error status.
  }
}

void TransportManager::handleSetParams(SetParamsOp&& op) {
  if (op.parameters.empty()) {
    // Nothing to apply; avoid the degenerate getParams({}, ...) which would enumerate every
    // parameter on every node.
    std::move(op.responder).respond({});
    return;
  }
  try {
    _paramBackend->setParams(op.parameters, std::chrono::seconds(5));
    // Fetch the actually-applied values so we can echo them back to the requester.
    std::vector<std::string_view> names;
    names.reserve(op.parameters.size());
    for (const auto& param : op.parameters) {
      names.emplace_back(param.name());
    }
    auto updated = _paramBackend->getParams(names, std::chrono::seconds(5));
    std::move(op.responder).respond(std::move(updated));
  } catch (const std::exception& ex) {
    _log.log(BridgeLogLevel::Error, "setParams failed: %s", ex.what());
    // Dropping the responder sends the client an error status.
  }
}

void TransportManager::handleSubscribeParams(SubscribeParamsOp&& op) {
  std::vector<std::string_view> toForward;
  toForward.reserve(op.names.size());
  for (const auto& name : op.names) {
    if (++_paramSubscriberCount[name] == 1) {
      toForward.emplace_back(name);
    }
  }
  if (toForward.empty()) {
    return;
  }
  try {
    _paramBackend->subscribeParams(toForward);
  } catch (const std::exception& ex) {
    _log.log(BridgeLogLevel::Error, "subscribeParams failed: %s", ex.what());
  }
}

void TransportManager::handleUnsubscribeParams(UnsubscribeParamsOp&& op) {
  std::vector<std::string_view> toForward;
  toForward.reserve(op.names.size());
  for (const auto& name : op.names) {
    auto it = _paramSubscriberCount.find(name);
    if (it == _paramSubscriberCount.end()) {
      _log.log(BridgeLogLevel::Warn, "Unsubscribe for untracked parameter '%s'", name.c_str());
      continue;
    }
    if (--it->second == 0) {
      toForward.emplace_back(name);  // `name` lives in `op.names` until this function returns
      _paramSubscriberCount.erase(it);
    }
  }
  if (toForward.empty()) {
    return;
  }
  try {
    _paramBackend->unsubscribeParams(toForward);
  } catch (const std::exception& ex) {
    _log.log(BridgeLogLevel::Error, "unsubscribeParams failed: %s", ex.what());
  }
}

}  // namespace foxglove_bridge
