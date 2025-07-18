#include <unordered_set>

#include <resource_retriever/retriever.hpp>

#include <foxglove_bridge/ros2_foxglove_bridge.hpp>

namespace foxglove_bridge {
namespace {
inline bool isHiddenTopicOrService(const std::string& name) {
  if (name.empty()) {
    throw std::invalid_argument("Topic or service name can't be empty");
  }
  return name.front() == '_' || name.find("/_") != std::string::npos;
}

inline foxglove::WebSocketServerCapabilities processCapabilities(
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
  foxglove::WebSocketServerCapabilities out = foxglove::WebSocketServerCapabilities(0);
  for (const auto& capability : capabilities) {
    if (STRING_TO_CAPABILITY.find(capability) != STRING_TO_CAPABILITY.end()) {
      out = out | STRING_TO_CAPABILITY.at(capability);
    }
  }
  return out;
}
}  // namespace

using namespace std::chrono_literals;
using namespace std::placeholders;
using foxglove_ws::isWhitelisted;

FoxgloveBridge::FoxgloveBridge(const rclcpp::NodeOptions& options)
    : Node("foxglove_bridge", options) {
  const char* rosDistro = std::getenv("ROS_DISTRO");
  RCLCPP_INFO(this->get_logger(), "Starting foxglove_bridge (%s, %s@%s) with %s", rosDistro,
              foxglove::FOXGLOVE_BRIDGE_VERSION, foxglove::FOXGLOVE_BRIDGE_GIT_HASH,
              foxglove::WebSocketUserAgent());

  declareParameters(this);

  const auto port = static_cast<uint16_t>(this->get_parameter(PARAM_PORT).as_int());
  const auto address = this->get_parameter(PARAM_ADDRESS).as_string();
  const auto send_buffer_limit =
    static_cast<size_t>(this->get_parameter(PARAM_SEND_BUFFER_LIMIT).as_int());
  const auto useTLS = this->get_parameter(PARAM_USETLS).as_bool();
  const auto certfile = this->get_parameter(PARAM_CERTFILE).as_string();
  const auto keyfile = this->get_parameter(PARAM_KEYFILE).as_string();
  _minQosDepth = static_cast<size_t>(this->get_parameter(PARAM_MIN_QOS_DEPTH).as_int());
  _maxQosDepth = static_cast<size_t>(this->get_parameter(PARAM_MAX_QOS_DEPTH).as_int());
  const auto bestEffortQosTopicWhiteList =
    this->get_parameter(PARAM_BEST_EFFORT_QOS_TOPIC_WHITELIST).as_string_array();
  _bestEffortQosTopicWhiteListPatterns = parseRegexStrings(this, bestEffortQosTopicWhiteList);
  const auto topicWhiteList = this->get_parameter(PARAM_TOPIC_WHITELIST).as_string_array();
  _topicWhitelistPatterns = parseRegexStrings(this, topicWhiteList);
  const auto serviceWhiteList = this->get_parameter(PARAM_SERVICE_WHITELIST).as_string_array();
  _serviceWhitelistPatterns = parseRegexStrings(this, serviceWhiteList);
  const auto paramWhiteList = this->get_parameter(PARAM_PARAMETER_WHITELIST).as_string_array();
  const auto paramWhitelistPatterns = parseRegexStrings(this, paramWhiteList);
  const auto useCompression = this->get_parameter(PARAM_USE_COMPRESSION).as_bool();
  _useSimTime = this->get_parameter("use_sim_time").as_bool();
  _capabilities = this->get_parameter(PARAM_CAPABILITIES).as_string_array();
  const auto clientTopicWhiteList =
    this->get_parameter(PARAM_CLIENT_TOPIC_WHITELIST).as_string_array();
  const auto clientTopicWhiteListPatterns = parseRegexStrings(this, clientTopicWhiteList);
  _includeHidden = this->get_parameter(PARAM_INCLUDE_HIDDEN).as_bool();
  const auto assetUriAllowlist = this->get_parameter(PARAM_ASSET_URI_ALLOWLIST).as_string_array();
  _assetUriAllowlistPatterns = parseRegexStrings(this, assetUriAllowlist);
  _disableLoanMessage = this->get_parameter(PARAM_DISABLE_LOAN_MESSAGE).as_bool();
  // const auto ignoreUnresponsiveParamNodes =
  //   this->get_parameter(PARAM_IGN_UNRESPONSIVE_PARAM_NODES).as_bool();

  const auto logHandler = std::bind(&FoxgloveBridge::logHandler, this, _1, _2);
  // Fetching of assets may be blocking, hence we fetch them in a separate thread.
  _fetchAssetQueue = std::make_unique<foxglove_ws::CallbackQueue>(logHandler, 1 /* num_threads */);

  foxglove_ws::ServerOptions serverOptions;
  serverOptions.capabilities = _capabilities;
  if (_useSimTime) {
    serverOptions.capabilities.push_back(foxglove_ws::CAPABILITY_TIME);
  }
  serverOptions.supportedEncodings = {"cdr", "json"};
  serverOptions.metadata = {{"ROS_DISTRO", rosDistro}};
  serverOptions.sendBufferLimitBytes = send_buffer_limit;
  serverOptions.sessionId = std::to_string(std::time(nullptr));
  serverOptions.useCompression = useCompression;
  serverOptions.useTls = useTLS;
  serverOptions.certfile = certfile;
  serverOptions.keyfile = keyfile;
  serverOptions.clientTopicWhitelistPatterns = clientTopicWhiteListPatterns;

  _server = foxglove_ws::ServerFactory::createServer<ConnectionHandle>("foxglove_bridge",
                                                                       logHandler, serverOptions);

  foxglove::setLogLevel(foxglove::LogLevel::Debug);
  foxglove::WebSocketServerOptions sdkServerOptions;
  sdkServerOptions.host = address;
  sdkServerOptions.port = port;
  sdkServerOptions.supported_encodings = {"cdr", "json"};
  sdkServerOptions.capabilities = processCapabilities(_capabilities);
  if (_useSimTime) {
    sdkServerOptions.capabilities =
      sdkServerOptions.capabilities | foxglove::WebSocketServerCapabilities::Time;
  }

  // Setup callbacks
  sdkServerOptions.callbacks.onConnectionGraphSubscribe =
    std::bind(&FoxgloveBridge::subscribeConnectionGraph, this, true);
  sdkServerOptions.callbacks.onSubscribe = std::bind(&FoxgloveBridge::subscribe, this, _1, _2);
  sdkServerOptions.callbacks.onUnsubscribe = std::bind(&FoxgloveBridge::unsubscribe, this, _1, _2);

  if (hasCapability(foxglove_ws::CAPABILITY_CLIENT_PUBLISH)) {
    sdkServerOptions.callbacks.onClientAdvertise =
      std::bind(&FoxgloveBridge::clientAdvertise, this, _1, _2);
    sdkServerOptions.callbacks.onClientUnadvertise =
      std::bind(&FoxgloveBridge::clientUnadvertise, this, _1, _2);
    sdkServerOptions.callbacks.onMessageData =
      std::bind(&FoxgloveBridge::clientMessage, this, _1, _2, _3, _4);
  }

  if (hasCapability(foxglove_ws::CAPABILITY_ASSETS)) {
    sdkServerOptions.fetch_asset = std::bind(&FoxgloveBridge::fetchAsset, this, _1, _2);
  }

  // TODO: The SDK server currently doesn't implement any of the TLS functionality. Once that
  // exists, add it here.

  auto maybeSdkServer = foxglove::WebSocketServer::create(std::move(sdkServerOptions));
  assert(maybeSdkServer.has_value());

  // Constructing an SDK server also starts it listening automatically
  _sdkServer = std::make_unique<foxglove::WebSocketServer>(std::move(maybeSdkServer.value()));
  this->set_parameter(rclcpp::Parameter{PARAM_PORT, _sdkServer->port()});
  RCLCPP_INFO(this->get_logger(), "[SDK] server listening on port %d", _sdkServer->port());

  foxglove_ws::ServerHandlers<ConnectionHandle> hdlrs;
  /*
  hdlrs.subscribeHandler = std::bind(&FoxgloveBridge::subscribe, this, _1, _2);
  hdlrs.unsubscribeHandler = std::bind(&FoxgloveBridge::unsubscribe, this, _1, _2);
  hdlrs.clientAdvertiseHandler = std::bind(&FoxgloveBridge::clientAdvertise, this, _1, _2);
  hdlrs.clientUnadvertiseHandler = std::bind(&FoxgloveBridge::clientUnadvertise, this, _1, _2);
  hdlrs.clientMessageHandler = std::bind(&FoxgloveBridge::clientMessage, this, _1, _2);
  hdlrs.serviceRequestHandler = std::bind(&FoxgloveBridge::serviceRequest, this, _1, _2);
  hdlrs.subscribeConnectionGraphHandler =
    std::bind(&FoxgloveBridge::subscribeConnectionGraph, this, _1);

  if (hasCapability(foxglove_ws::CAPABILITY_PARAMETERS) ||
      hasCapability(foxglove_ws::CAPABILITY_PARAMETERS_SUBSCRIBE)) {
    hdlrs.parameterRequestHandler = std::bind(&FoxgloveBridge::getParameters, this, _1, _2, _3);
    hdlrs.parameterChangeHandler = std::bind(&FoxgloveBridge::setParameters, this, _1, _2, _3);
    hdlrs.parameterSubscriptionHandler =
      std::bind(&FoxgloveBridge::subscribeParameters, this, _1, _2, _3);

    _paramInterface = std::make_shared<ParameterInterface>(this, paramWhitelistPatterns,
                                                           ignoreUnresponsiveParamNodes
                                                             ? UnresponsiveNodePolicy::Ignore
                                                             : UnresponsiveNodePolicy::Retry);
    _paramInterface->setParamUpdateCallback(std::bind(&FoxgloveBridge::parameterUpdates, this, _1));
  }

  if (hasCapability(foxglove_ws::CAPABILITY_ASSETS)) {
    hdlrs.fetchAssetHandler = [this](const std::string& uri, uint32_t requestId,
                                     ConnectionHandle hdl) {
      _fetchAssetQueue->addCallback(
        std::bind(&FoxgloveBridge::fetchAsset, this, uri, requestId, hdl));
    };
  }
  _server->setHandlers(std::move(hdlrs));
  */

  // Start the thread polling for rosgraph changes
  _rosgraphPollThread =
    std::make_unique<std::thread>(std::bind(&FoxgloveBridge::rosgraphPollThread, this));

  _subscriptionCallbackGroup = this->create_callback_group(rclcpp::CallbackGroupType::Reentrant);
  _clientPublishCallbackGroup =
    this->create_callback_group(rclcpp::CallbackGroupType::MutuallyExclusive);
  _servicesCallbackGroup = this->create_callback_group(rclcpp::CallbackGroupType::Reentrant);

  if (_useSimTime) {
    _clockSubscription = this->create_subscription<rosgraph_msgs::msg::Clock>(
      "/clock", rclcpp::QoS{rclcpp::KeepLast(1)}.best_effort(),
      [&](std::shared_ptr<const rosgraph_msgs::msg::Clock> msg) {
        const auto timestamp = rclcpp::Time{msg->clock}.nanoseconds();
        assert(timestamp >= 0 && "Timestamp is negative");
        _sdkServer->broadcastTime(static_cast<uint64_t>(timestamp));
      });
  }
}

FoxgloveBridge::~FoxgloveBridge() {
  _shuttingDown = true;
  RCLCPP_INFO(this->get_logger(), "Shutting down %s", this->get_name());
  if (_rosgraphPollThread) {
    _rosgraphPollThread->join();
  }
  _server->stop();
  _sdkServer->stop();
  RCLCPP_INFO(this->get_logger(), "Shutdown complete");
}

void FoxgloveBridge::rosgraphPollThread() {
  updateAdvertisedTopics(get_topic_names_and_types());
  updateAdvertisedServices();

  auto graphEvent = this->get_graph_event();
  while (!_shuttingDown && rclcpp::ok()) {
    try {
      this->wait_for_graph_change(graphEvent, 200ms);
      bool triggered = graphEvent->check_and_clear();
      if (triggered) {
        RCLCPP_DEBUG(this->get_logger(), "rosgraph change detected");
        const auto topicNamesAndTypes = get_topic_names_and_types();
        updateAdvertisedTopics(topicNamesAndTypes);
        updateAdvertisedServices();
        if (_subscribeGraphUpdates) {
          updateConnectionGraph(topicNamesAndTypes);
        }
        // Graph changes tend to come in batches, so wait a bit before checking again
        std::this_thread::sleep_for(500ms);
      }
    } catch (const std::exception& ex) {
      RCLCPP_ERROR(this->get_logger(), "Exception thrown in rosgraphPollThread: %s", ex.what());
    }
  }

  RCLCPP_DEBUG(this->get_logger(), "rosgraph polling thread exiting");
}

void FoxgloveBridge::updateAdvertisedTopics(
  const std::map<std::string, std::vector<std::string>>& topicNamesAndTypes) {
  if (!rclcpp::ok()) {
    return;
  }

  std::unordered_set<TopicAndDatatype, PairHash> latestTopics;
  latestTopics.reserve(topicNamesAndTypes.size());
  for (const auto& topicNamesAndType : topicNamesAndTypes) {
    const auto& topicName = topicNamesAndType.first;
    const auto& datatypes = topicNamesAndType.second;

    // Ignore hidden topics if not explicitly included
    if (!_includeHidden && isHiddenTopicOrService(topicName)) {
      continue;
    }

    // Ignore the topic if it is not on the topic whitelist
    if (isWhitelisted(topicName, _topicWhitelistPatterns)) {
      for (const auto& datatype : datatypes) {
        latestTopics.emplace(topicName, datatype);
      }
    }
  }

  if (const auto numIgnoredTopics = topicNamesAndTypes.size() - latestTopics.size()) {
    RCLCPP_DEBUG(
      this->get_logger(),
      "%zu topics have been ignored as they do not match any pattern on the topic whitelist",
      numIgnoredTopics);
  }

  std::lock_guard<std::mutex> lock(_subscriptionsMutex);

  // Remove channels for which the topic does not exist anymore
  for (auto channelIt = _sdkChannels.begin(); channelIt != _sdkChannels.end();) {
    auto& channel = channelIt->second;
    std::string schemaName = channel.schema().value().name;
    std::string topic(channel.topic());
    const TopicAndDatatype topicAndSchemaName = {topic, schemaName};
    if (latestTopics.find(topicAndSchemaName) == latestTopics.end()) {
      RCLCPP_INFO(this->get_logger(), "Removing channel %lu for topic \"%s\" (%s)", channel.id(),
                  topic.c_str(), schemaName.c_str());
      channel.close();
      channelIt = _sdkChannels.erase(channelIt);
    } else {
      channelIt++;
    }
  }

  // Advertise new topics
  for (const auto& topicAndDatatype : latestTopics) {
    const auto& topic = topicAndDatatype.first;
    const auto& schemaName = topicAndDatatype.second;

    if (std::find_if(
          _sdkChannels.begin(), _sdkChannels.end(), [&topic, &schemaName](const auto& kvp) {
            const auto& [channelId, channel] = kvp;
            return channel.topic() == topic && channel.schema().value().name == schemaName;
          }) != _sdkChannels.end()) {
      continue;
    }

    // Load actual schema and encoding from disk
    // TODO: (FG-10638): Add support for reading schemas from the wire if available
    std::optional<foxglove::Schema> schema = foxglove::Schema();
    schema->name = schemaName;
    std::string messageEncoding;

    try {
      auto [format, msgDefinition] = _messageDefinitionCache.get_full_text(schemaName);
      schema->data_len = msgDefinition.size();
      schema->data = reinterpret_cast<const std::byte*>(msgDefinition.data());

      switch (format) {
        case foxglove::MessageDefinitionFormat::MSG:
          messageEncoding = "cdr";
          schema->encoding = "ros2msg";
          break;
        case foxglove::MessageDefinitionFormat::IDL:
          messageEncoding = "cdr";
          schema->encoding = "ros2idl";
          break;
        default:
          RCLCPP_WARN(this->get_logger(), "Unsupported message definition format for type %s",
                      schemaName.c_str());
          continue;
      }
    } catch (const foxglove::DefinitionNotFoundError& err) {
      // If the definition isn't found, advertise the channel with an empty schema as a fallback
      RCLCPP_WARN(this->get_logger(), "Could not find definition for type %s: %s",
                  schemaName.c_str(), err.what());
      schema = std::nullopt;
    } catch (const std::exception& err) {
      RCLCPP_ERROR(this->get_logger(), "Failed to load schemaDefinition for topic \"%s\" (%s): %s",
                   topic.c_str(), schemaName.c_str(), err.what());
      continue;
    }

    // Create the new SDK channel
    auto channelResult = foxglove::RawChannel::create(topic, messageEncoding, schema);
    if (!channelResult.has_value()) {
      RCLCPP_ERROR(this->get_logger(), "Failed to create channel for topic \"%s\" (%s)",
                   topic.c_str(), foxglove::strerror(channelResult.error()));
      continue;
    }

    const ChannelId channelId = channelResult.value().id();
    RCLCPP_INFO(this->get_logger(), "Advertising new channel %lu for topic \"%s\"", channelId,
                topic.c_str());
    _sdkChannels.insert({channelId, std::move(channelResult.value())});
  }
}

void FoxgloveBridge::updateAdvertisedServices() {
  if (!rclcpp::ok()) {
    return;
  } else if (!hasCapability(foxglove_ws::CAPABILITY_SERVICES)) {
    return;
  }

  // Get the current list of visible services and datatypes from the ROS graph
  const auto serviceNamesAndTypes = this->get_node_graph_interface()->get_service_names_and_types();

  std::lock_guard<std::mutex> lock(_servicesMutex);

  // Remove advertisements for services that have been removed
  std::vector<foxglove_ws::ServiceId> servicesToRemove;
  for (const auto& service : _advertisedServices) {
    const auto it = std::find_if(serviceNamesAndTypes.begin(), serviceNamesAndTypes.end(),
                                 [service](const auto& serviceNameAndTypes) {
                                   return serviceNameAndTypes.first == service.second.name;
                                 });
    if (it == serviceNamesAndTypes.end()) {
      servicesToRemove.push_back(service.first);
    }
  }
  for (auto serviceId : servicesToRemove) {
    _advertisedServices.erase(serviceId);
  }
  _server->removeServices(servicesToRemove);

  // Advertise new services
  std::vector<foxglove_ws::ServiceWithoutId> newServices;
  for (const auto& serviceNamesAndType : serviceNamesAndTypes) {
    const auto& serviceName = serviceNamesAndType.first;
    const auto& datatypes = serviceNamesAndType.second;

    // Ignore the service if it's already advertised
    if (std::find_if(_advertisedServices.begin(), _advertisedServices.end(),
                     [serviceName](const auto& idWithService) {
                       return idWithService.second.name == serviceName;
                     }) != _advertisedServices.end()) {
      continue;
    }

    // Ignore hidden services if not explicitly included
    if (!_includeHidden && isHiddenTopicOrService(serviceName)) {
      continue;
    }

    // Ignore the service if it is not on the service whitelist
    if (!isWhitelisted(serviceName, _serviceWhitelistPatterns)) {
      continue;
    }

    foxglove_ws::ServiceWithoutId service;
    service.name = serviceName;
    service.type = datatypes.front();

    try {
      const auto requestTypeName = service.type + foxglove::SERVICE_REQUEST_MESSAGE_SUFFIX;
      const auto responseTypeName = service.type + foxglove::SERVICE_RESPONSE_MESSAGE_SUFFIX;
      const auto [format, reqSchema] = _messageDefinitionCache.get_full_text(requestTypeName);
      const auto resSchema = _messageDefinitionCache.get_full_text(responseTypeName).second;
      switch (format) {
        case foxglove::MessageDefinitionFormat::MSG:
          service.requestSchema = reqSchema;
          service.responseSchema = resSchema;
          break;
        case foxglove::MessageDefinitionFormat::IDL:
          RCLCPP_WARN(this->get_logger(),
                      "IDL message definition format cannot be communicated over ws-protocol. "
                      "Service \"%s\" (%s) may not decode correctly in clients",
                      service.name.c_str(), service.type.c_str());
          service.requestSchema = reqSchema;
          service.responseSchema = resSchema;
          break;
      }
    } catch (const foxglove::DefinitionNotFoundError& err) {
      RCLCPP_WARN(this->get_logger(), "Could not find definition for type %s: %s",
                  service.type.c_str(), err.what());
      // We still advertise the service, but with an emtpy schema
      service.requestSchema = "";
      service.responseSchema = "";
    } catch (const std::exception& err) {
      RCLCPP_WARN(this->get_logger(), "Failed to add service \"%s\" (%s): %s", service.name.c_str(),
                  service.type.c_str(), err.what());
      continue;
    }

    newServices.push_back(service);
  }

  const auto serviceIds = _server->addServices(newServices);
  for (size_t i = 0; i < serviceIds.size(); ++i) {
    _advertisedServices.emplace(serviceIds[i], newServices[i]);
  }
}

void FoxgloveBridge::updateConnectionGraph(
  const std::map<std::string, std::vector<std::string>>& topicNamesAndTypes) {
  MapOfSets publishers, subscribers;
  foxglove::ConnectionGraph connectionGraph;

  for (const auto& topicNameAndType : topicNamesAndTypes) {
    const auto& topicName = topicNameAndType.first;
    if (!isWhitelisted(topicName, _topicWhitelistPatterns)) {
      continue;
    }

    const auto publishersInfo = get_publishers_info_by_topic(topicName);
    const auto subscribersInfo = get_subscriptions_info_by_topic(topicName);
    std::unordered_set<std::string> publisherIds, subscriberIds;
    for (const auto& publisher : publishersInfo) {
      const auto& ns = publisher.node_namespace();
      const auto sep = (!ns.empty() && ns.back() == '/') ? "" : "/";
      publisherIds.insert(ns + sep + publisher.node_name());
    }
    for (const auto& subscriber : subscribersInfo) {
      const auto& ns = subscriber.node_namespace();
      const auto sep = (!ns.empty() && ns.back() == '/') ? "" : "/";
      subscriberIds.insert(ns + sep + subscriber.node_name());
    }
    publishers.emplace(topicName, publisherIds);
    subscribers.emplace(topicName, subscriberIds);

    std::vector<std::string> publisherIdsVec(publisherIds.begin(), publisherIds.end());
    std::vector<std::string> subscriberIdsVec(subscriberIds.begin(), subscriberIds.end());
    connectionGraph.setPublishedTopic(topicName, publisherIdsVec);
    connectionGraph.setSubscribedTopic(topicName, subscriberIdsVec);
  }

  MapOfSets services;
  for (const auto& fqnNodeName : get_node_names()) {
    const auto [nodeNs, nodeName] = getNodeAndNodeNamespace(fqnNodeName);
    const auto serviceNamesAndTypes = get_service_names_and_types_by_node(nodeName, nodeNs);

    for (const auto& [serviceName, serviceTypes] : serviceNamesAndTypes) {
      (void)serviceTypes;
      if (isWhitelisted(serviceName, _serviceWhitelistPatterns)) {
        services[serviceName].insert(fqnNodeName);
      }
    }
  }
  for (const auto& [serviceName, providerIds] : services) {
    connectionGraph.setAdvertisedService(serviceName,
                                         std::vector(providerIds.begin(), providerIds.end()));
  }

  RCLCPP_INFO(this->get_logger(), "[SDK] publishing connection graph");
  _sdkServer->publishConnectionGraph(connectionGraph);
}

void FoxgloveBridge::subscribeConnectionGraph(bool subscribe) {
  RCLCPP_INFO(this->get_logger(), "[SDK] received connection graph subscribe request");
  if ((_subscribeGraphUpdates = subscribe)) {
    // TODO: This causes a deadlock in the SDK implementation
    // updateConnectionGraph(get_topic_names_and_types());
  };
}

void FoxgloveBridge::subscribe(ChannelId channelId, const foxglove::ClientMetadata& client) {
  if (!client.sink_id.has_value()) {
    RCLCPP_ERROR(this->get_logger(),
                 "[SDK] received subscribe request from client %u for channel %lu but client "
                 "has no sink ID",
                 client.id, channelId);
    return;
  }

  RCLCPP_INFO(this->get_logger(),
              "[SDK] received subscribe request for channel %lu from client %u (sink %lu)",
              channelId, client.id, client.sink_id.value());
  std::lock_guard<std::mutex> lock(_subscriptionsMutex);

  // REVIEW: Is this necessary if the SDK server is checking that the channel exists before calling
  // this callback?
  auto it = _sdkChannels.find(channelId);
  if (it == _sdkChannels.end()) {
    RCLCPP_ERROR(this->get_logger(), "[SDK] received subscribe request for unknown channel: %lu",
                 channelId);
    return;
  }

  auto& channel = it->second;
  const std::string topic(channel.topic());
  const std::string datatype = channel.schema().value().name;

  const rclcpp::QoS qos = determineQoS(topic);

  rclcpp::SubscriptionEventCallbacks eventCallbacks;
  eventCallbacks.incompatible_qos_callback = [&](const rclcpp::QOSRequestedIncompatibleQoSInfo&) {
    RCLCPP_ERROR(this->get_logger(), "Incompatible subscriber QoS settings for topic \"%s\" (%s)",
                 topic.c_str(), datatype.c_str());
  };

  rclcpp::SubscriptionOptions subscriptionOptions;
  subscriptionOptions.event_callbacks = eventCallbacks;
  subscriptionOptions.callback_group = _subscriptionCallbackGroup;

  auto subscription = this->create_generic_subscription(
    topic, datatype, qos,
    [this, channelId, client](std::shared_ptr<const rclcpp::SerializedMessage> msg) {
      this->rosMessageHandler(channelId, client.sink_id.value(), msg);
    },
    subscriptionOptions);

  if (!client.sink_id.has_value()) {
    RCLCPP_ERROR(this->get_logger(),
                 "[SDK] received subscribe request for channel %lu but client "
                 "has no sink ID",
                 channelId);
    return;
  }

  _sdkSubscriptions.insert({{channelId, client.id}, subscription});
  RCLCPP_INFO(
    this->get_logger(),
    "[SDK] created ROS subscription on %s (%s) successfully for channel %lu (client %u, sink %lu)",
    topic.c_str(), datatype.c_str(), channelId, client.id, client.sink_id.value());
}

void FoxgloveBridge::unsubscribe(ChannelId channelId, const foxglove::ClientMetadata& client) {
  std::lock_guard<std::mutex> lock(_subscriptionsMutex);

  RCLCPP_INFO(this->get_logger(), "[SDK] received unsubscribe request for channel %lu", channelId);

  auto it = _sdkChannels.find(channelId);
  if (it == _sdkChannels.end()) {
    RCLCPP_ERROR(this->get_logger(), "[SDK] received unsubscribe request for unknown channel %lu",
                 channelId);
    return;
  }

  auto subscriptionIt = _sdkSubscriptions.find({channelId, client.id});
  if (subscriptionIt == _sdkSubscriptions.end()) {
    RCLCPP_ERROR(this->get_logger(),
                 "[SDK] Client %u tried unsubscribing from channel %lu but a corresponding ROS "
                 "subscription doesn't exist",
                 client.id, channelId);
    return;
  }

  const std::string& topic = subscriptionIt->second->get_topic_name();
  RCLCPP_INFO(this->get_logger(),
              "[SDK] Cleaned up subscription to topic %s for client %u on channel %lu",
              topic.c_str(), client.id, channelId);
  _sdkSubscriptions.erase(subscriptionIt);
}

void FoxgloveBridge::clientAdvertise(ClientId clientId, const foxglove::ClientChannel& channel) {
  std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

  ChannelAndClientId key = {channel.id, clientId};
  const std::string topicName(channel.topic);
  const std::string topicType(channel.schema_name);
  const std::string encoding(channel.encoding);

  if (_clientAdvertisedTopics.find(key) != _clientAdvertisedTopics.end()) {
    throw foxglove_ws::ClientChannelError(
      channel.id, "Received client advertisement from client ID " + std::to_string(clientId) +
                    " for channel " + std::to_string(channel.id) + " it had already advertised");
  }

  if (channel.schema_name.empty()) {
    throw foxglove_ws::ClientChannelError(
      channel.id, "Received client advertisement from client ID " + std::to_string(clientId) +
                    " for channel " + std::to_string(channel.id) + " with empty schema name");
  }

  if (encoding == "json") {
    // register the JSON parser for this schemaName
    std::string schemaName(channel.schema_name);
    auto parserIt = _jsonParsers.find(schemaName);
    if (parserIt == _jsonParsers.end()) {
      std::string schema = "";
      if (channel.schema_len > 0) {
        // Schema is given by the advertisement
        schema = std::string(reinterpret_cast<const char*>(channel.schema), channel.schema_len);
      } else {
        // Schema not given, look it up.
        auto [format, msgDefinition] = _messageDefinitionCache.get_full_text(schemaName);
        if (format != foxglove::MessageDefinitionFormat::MSG) {
          throw foxglove_ws::ClientChannelError(
            channel.id, "Message definition (.msg) for schema " + schemaName + " not found.");
        }

        schema = msgDefinition;
      }

      auto parser = std::make_shared<RosMsgParser::Parser>(
        topicName, RosMsgParser::ROSType(schemaName), schema);
      _jsonParsers.insert({schemaName, parser});
    }
  }

  try {
    // Create a new topic advertisement

    // Lookup if there are publishers from other nodes for that topic. If that's the case, we
    // use a matching QoS profile.
    const auto otherPublishers = get_publishers_info_by_topic(topicName);
    const auto otherPublisherIt =
      std::find_if(otherPublishers.begin(), otherPublishers.end(),
                   [this](const rclcpp::TopicEndpointInfo& endpoint) {
                     return endpoint.node_name() != this->get_name() ||
                            endpoint.node_namespace() != this->get_namespace();
                   });
    rclcpp::QoS qos = otherPublisherIt == otherPublishers.end() ? rclcpp::SystemDefaultsQoS()
                                                                : otherPublisherIt->qos_profile();

    // When the QoS profile is copied from another existing publisher, it can happen that the
    // history policy is Unknown, leading to an error when subsequently trying to create a
    // publisher with that QoS profile. As a fix, we explicitly set the history policy to the
    // system default.
    if (qos.history() == rclcpp::HistoryPolicy::Unknown) {
      qos.history(rclcpp::HistoryPolicy::SystemDefault);
    }
    rclcpp::PublisherOptions publisherOptions{};
    publisherOptions.callback_group = _clientPublishCallbackGroup;
    auto publisher = this->create_generic_publisher(topicName, topicType, qos, publisherOptions);

    RCLCPP_INFO(this->get_logger(),
                "Client ID %d is advertising \"%s\" (%s) on channel %d with encoding \"%s\"",
                clientId, topicName.c_str(), topicType.c_str(), channel.id, encoding.c_str());

    // Store the new topic advertisement
    ClientAdvertisement clientAdvertisement{std::move(publisher), topicName, topicType, encoding};
    _clientAdvertisedTopics.emplace(key, std::move(clientAdvertisement));
  } catch (const std::exception& ex) {
    throw foxglove_ws::ClientChannelError(channel.id,
                                          std::string("Failed to create publisher: ") + ex.what());
  }
}

void FoxgloveBridge::clientUnadvertise(ClientId clientId, ChannelId clientChannelId) {
  std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

  ChannelAndClientId key = {clientChannelId, clientId};

  auto it = _clientAdvertisedTopics.find(key);
  if (it == _clientAdvertisedTopics.end()) {
    throw foxglove_ws::ClientChannelError(clientChannelId,
                                          "Ignoring client unadvertisement from client ID " +
                                            std::to_string(clientId) + " for unknown channel " +
                                            std::to_string(clientChannelId));
  }

  const auto& publisher = it->second.publisher;
  RCLCPP_INFO(this->get_logger(),
              "Client ID %u is no longer advertising %s (%zu subscribers) on channel %lu", clientId,
              publisher->get_topic_name(), publisher->get_subscription_count(), clientChannelId);

  _clientAdvertisedTopics.erase(it);

  // Create a timer that immedeately goes out of scope (so it never fires) which will trigger
  // the previously destroyed publisher to be cleaned up. This is a workaround for
  // https://github.com/ros2/rclcpp/issues/2146
  this->create_wall_timer(1s, []() {});
}

void FoxgloveBridge::clientMessage(ClientId clientId, ChannelId clientChannelId,
                                   const std::byte* data, size_t dataLen) {
  // Get the publisher
  rclcpp::GenericPublisher::SharedPtr publisher;
  std::string encoding;
  std::string schemaName;
  {
    const ChannelAndClientId key = {clientChannelId, clientId};
    std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

    auto it = _clientAdvertisedTopics.find(key);
    if (it == _clientAdvertisedTopics.end()) {
      throw foxglove_ws::ClientChannelError(
        clientChannelId, "Dropping client message from client ID " + std::to_string(clientId) +
                           " for unknown channel " + std::to_string(clientChannelId) +
                           ", client has no advertised topics");
    }

    publisher = it->second.publisher;
    encoding = it->second.encoding;
    schemaName = it->second.topicType;
  }

  auto publishMessage = [publisher, this](const void* data, size_t size) {
    // Copy the message payload into a SerializedMessage object
    rclcpp::SerializedMessage serializedMessage{size};
    auto& rclSerializedMsg = serializedMessage.get_rcl_serialized_message();
    std::memcpy(rclSerializedMsg.buffer, data, size);
    rclSerializedMsg.buffer_length = size;
    // Publish the message
    if (_disableLoanMessage || !publisher->can_loan_messages()) {
      publisher->publish(serializedMessage);
    } else {
      publisher->publish_as_loaned_msg(serializedMessage);
    }
  };

  if (encoding == "cdr") {
    publishMessage(data, dataLen);
  } else if (encoding == "json") {
    // get the specific parser for this schemaName
    std::shared_ptr<RosMsgParser::Parser> parser;
    {
      std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);
      auto parserIt = _jsonParsers.find(schemaName);
      if (parserIt != _jsonParsers.end()) {
        parser = parserIt->second;
      }
    }
    if (!parser) {
      throw foxglove_ws::ClientChannelError(
        clientChannelId, "Dropping client message from client ID " + std::to_string(clientId) +
                           " with encoding \"json\": no parser found");
    } else {
      thread_local RosMsgParser::ROS2_Serializer serializer;
      serializer.reset();
      const std::string jsonMessage(reinterpret_cast<const char*>(data), dataLen);
      try {
        parser->serializeFromJson(jsonMessage, &serializer);
        publishMessage(serializer.getBufferData(), serializer.getBufferSize());
      } catch (const std::exception& ex) {
        throw foxglove_ws::ClientChannelError(
          clientChannelId, "Dropping client message from client ID " + std::to_string(clientId) +
                             " with encoding \"json\": " + ex.what());
      }
    }
  } else {
    throw foxglove_ws::ClientChannelError(
      clientChannelId, "Dropping client message from client ID " + std::to_string(clientId) +
                         " with unknown encoding \"" + encoding + "\"");
  }
}

void FoxgloveBridge::setParameters(const std::vector<foxglove_ws::Parameter>& parameters,
                                   const std::optional<std::string>& requestId,
                                   ConnectionHandle hdl) {
  _paramInterface->setParams(parameters, std::chrono::seconds(5));

  // If a request Id was given, send potentially updated parameters back to client
  if (requestId) {
    std::vector<std::string> parameterNames(parameters.size());
    for (size_t i = 0; i < parameters.size(); ++i) {
      parameterNames[i] = parameters[i].getName();
    }
    getParameters(parameterNames, requestId, hdl);
  }
}

void FoxgloveBridge::getParameters(const std::vector<std::string>& parameters,
                                   const std::optional<std::string>& requestId,
                                   ConnectionHandle hdl) {
  const auto params = _paramInterface->getParams(parameters, std::chrono::seconds(5));
  _server->publishParameterValues(hdl, params, requestId);
}

void FoxgloveBridge::subscribeParameters(const std::vector<std::string>& parameters,
                                         foxglove_ws::ParameterSubscriptionOperation op,
                                         ConnectionHandle) {
  if (op == foxglove_ws::ParameterSubscriptionOperation::SUBSCRIBE) {
    _paramInterface->subscribeParams(parameters);
  } else {
    _paramInterface->unsubscribeParams(parameters);
  }
}

void FoxgloveBridge::parameterUpdates(const std::vector<foxglove_ws::Parameter>& parameters) {
  _server->updateParameterValues(parameters);
}

void FoxgloveBridge::logHandler(LogLevel level, char const* msg) {
  switch (level) {
    case LogLevel::Debug:
      RCLCPP_DEBUG(this->get_logger(), "[WS] %s", msg);
      break;
    case LogLevel::Info:
      RCLCPP_INFO(this->get_logger(), "[WS] %s", msg);
      break;
    case LogLevel::Warn:
      RCLCPP_WARN(this->get_logger(), "[WS] %s", msg);
      break;
    case LogLevel::Error:
      RCLCPP_ERROR(this->get_logger(), "[WS] %s", msg);
      break;
    case LogLevel::Critical:
      RCLCPP_FATAL(this->get_logger(), "[WS] %s", msg);
      break;
  }
}

void FoxgloveBridge::rosMessageHandler(ChannelId channelId, SinkId sinkId,
                                       std::shared_ptr<const rclcpp::SerializedMessage> msg) {
  // NOTE: Do not call any RCLCPP_* logging functions from this function. Otherwise, subscribing
  // to `/rosout` will cause a feedback loop
  const auto timestamp = this->now().nanoseconds();
  assert(timestamp >= 0 && "Timestamp is negative");
  const auto rclSerializedMsg = msg->get_rcl_serialized_message();

  std::lock_guard<std::mutex> lock(_subscriptionsMutex);
  if (_sdkChannels.find(channelId) == _sdkChannels.end()) {
    return;
  }

  auto& channel = _sdkChannels.at(channelId);
  channel.log(reinterpret_cast<const std::byte*>(rclSerializedMsg.buffer),
              rclSerializedMsg.buffer_length, timestamp, sinkId);
}

void FoxgloveBridge::serviceRequest(const foxglove_ws::ServiceRequest& request,
                                    ConnectionHandle clientHandle) {
  RCLCPP_DEBUG(this->get_logger(), "Received a request for service %d", request.serviceId);

  std::lock_guard<std::mutex> lock(_servicesMutex);
  const auto serviceIt = _advertisedServices.find(request.serviceId);
  if (serviceIt == _advertisedServices.end()) {
    throw foxglove_ws::ServiceError(
      request.serviceId,
      "Service with id " + std::to_string(request.serviceId) + " does not exist");
  }

  auto clientIt = _serviceClients.find(request.serviceId);
  if (clientIt == _serviceClients.end()) {
    try {
      auto clientOptions = rcl_client_get_default_options();
      auto genClient = GenericClient::make_shared(
        this->get_node_base_interface().get(), this->get_node_graph_interface(),
        serviceIt->second.name, serviceIt->second.type, clientOptions);
      clientIt = _serviceClients.emplace(request.serviceId, std::move(genClient)).first;
      this->get_node_services_interface()->add_client(clientIt->second, _servicesCallbackGroup);
    } catch (const std::exception& ex) {
      throw foxglove_ws::ServiceError(
        request.serviceId,
        "Failed to create service client for service " + serviceIt->second.name + ": " + ex.what());
    }
  }

  auto client = clientIt->second;
  if (!client->wait_for_service(1s)) {
    throw foxglove_ws::ServiceError(request.serviceId,
                                    "Service " + serviceIt->second.name + " is not available");
  }

  auto reqMessage = std::make_shared<rclcpp::SerializedMessage>(request.data.size());
  auto& rclSerializedMsg = reqMessage->get_rcl_serialized_message();
  std::memcpy(rclSerializedMsg.buffer, request.data.data(), request.data.size());
  rclSerializedMsg.buffer_length = request.data.size();

  auto responseReceivedCallback = [this, request,
                                   clientHandle](GenericClient::SharedFuture future) {
    const auto serializedResponseMsg = future.get()->get_rcl_serialized_message();
    foxglove_ws::ServiceRequest response{request.serviceId, request.callId, request.encoding,
                                         std::vector<uint8_t>(serializedResponseMsg.buffer_length)};
    std::memcpy(response.data.data(), serializedResponseMsg.buffer,
                serializedResponseMsg.buffer_length);
    _server->sendServiceResponse(clientHandle, response);
  };
  client->async_send_request(reqMessage, responseReceivedCallback);
}

void FoxgloveBridge::fetchAsset(const std::string_view uriView,
                                foxglove::FetchAssetResponder&& responder) {
  std::string uri(uriView);
  try {
    // We reject URIs that are not on the allowlist or that contain two consecutive dots. The
    // latter can be utilized to construct URIs for retrieving confidential files that should
    // not be accessible over the WebSocket connection. Example:
    // `package://<pkg_name>/../../../secret.txt`. This is an extra security measure and should
    // not be necessary if the allowlist is strict enough.
    if (uri.find("..") != std::string::npos || !isWhitelisted(uri, _assetUriAllowlistPatterns)) {
      throw std::runtime_error("Asset URI not allowed: " + uri);
    }

    resource_retriever::Retriever resource_retriever;

    // The resource_retriever API has changed from 3.7 onwards.
#if RESOURCE_RETRIEVER_VERSION_MAJOR > 3 || \
  (RESOURCE_RETRIEVER_VERSION_MAJOR == 3 && RESOURCE_RETRIEVER_VERSION_MINOR > 6)
    const auto memoryResource = resource_retriever.get_shared(uri);
    std::vector<std::byte> data(memoryResource->data.size());
    std::memcpy(data.data(), memoryResource->data.data(), memoryResource->data.size());
    std::move(responder).respondOk(data);
#else
    const resource_retriever::MemoryResource memoryResource = resource_retriever.get(uri);
    std::vector<std::byte> data(memoryResource.size);
    std::memcpy(data.data(), memoryResource.data.get(), memoryResource.size);
    std::move(responder).respondOk(data);
#endif
  } catch (const std::exception& ex) {
    RCLCPP_WARN(this->get_logger(), "Failed to retrieve asset '%s': %s", uri.c_str(), ex.what());
    std::move(responder).respondError("Failed to retrieve asset " + uri);
  }
}

bool FoxgloveBridge::hasCapability(const std::string& capability) {
  return std::find(_capabilities.begin(), _capabilities.end(), capability) != _capabilities.end();
}

rclcpp::QoS FoxgloveBridge::determineQoS(const std::string& topic) {
  // Select an appropriate subscription QOS profile. This is similar to how ros2 topic echo
  // does it:
  // https://github.com/ros2/ros2cli/blob/619b3d1c9/ros2topic/ros2topic/verb/echo.py#L137-L194
  size_t depth = 0;
  size_t reliabilityReliableEndpointsCount = 0;
  size_t durabilityTransientLocalEndpointsCount = 0;

  const auto publisherInfo = this->get_publishers_info_by_topic(topic);
  for (const auto& publisher : publisherInfo) {
    const auto& qos = publisher.qos_profile();
    if (qos.reliability() == rclcpp::ReliabilityPolicy::Reliable) {
      ++reliabilityReliableEndpointsCount;
    }
    if (qos.durability() == rclcpp::DurabilityPolicy::TransientLocal) {
      ++durabilityTransientLocalEndpointsCount;
    }
    // Some RMWs do not retrieve history information of the publisher endpoint in which case the
    // history depth is 0. We use a lower limit of 1 here, so that the history depth is at least
    // equal to the number of publishers. This covers the case where there are multiple
    // transient_local publishers with a depth of 1 (e.g. multiple tf_static transform
    // broadcasters). See also
    // https://github.com/foxglove/ros-foxglove-bridge/issues/238 and
    // https://github.com/foxglove/ros-foxglove-bridge/issues/208
    const size_t publisherHistoryDepth = std::max(static_cast<size_t>(1), qos.depth());
    depth = depth + publisherHistoryDepth;
  }

  depth = std::max(depth, _minQosDepth);
  if (depth > _maxQosDepth) {
    RCLCPP_WARN(this->get_logger(),
                "Limiting history depth for topic '%s' to %zu (was %zu). You may want to increase "
                "the max_qos_depth parameter value.",
                topic.c_str(), _maxQosDepth, depth);
    depth = _maxQosDepth;
  }

  rclcpp::QoS qos{rclcpp::KeepLast(depth)};

  // Force the QoS to be "best_effort" if in the whitelist
  if (isWhitelisted(topic, _bestEffortQosTopicWhiteListPatterns)) {
    qos.best_effort();
  } else if (!publisherInfo.empty() && reliabilityReliableEndpointsCount == publisherInfo.size()) {
    // If all endpoints are reliable, ask for reliable
    qos.reliable();
  } else {
    if (reliabilityReliableEndpointsCount > 0) {
      RCLCPP_WARN(
        this->get_logger(),
        "Some, but not all, publishers on topic '%s' are offering "
        "QoSReliabilityPolicy.RELIABLE. "
        "Falling back to QoSReliabilityPolicy.BEST_EFFORT as it will connect to all publishers",
        topic.c_str());
    }
    qos.best_effort();
  }

  // If all endpoints are transient_local, ask for transient_local
  if (!publisherInfo.empty() && durabilityTransientLocalEndpointsCount == publisherInfo.size()) {
    qos.transient_local();
  } else {
    if (durabilityTransientLocalEndpointsCount > 0) {
      RCLCPP_WARN(this->get_logger(),
                  "Some, but not all, publishers on topic '%s' are offering "
                  "QoSDurabilityPolicy.TRANSIENT_LOCAL. Falling back to "
                  "QoSDurabilityPolicy.VOLATILE as it will connect to all publishers",
                  topic.c_str());
    }
    qos.durability_volatile();
  }

  return qos;
}

}  // namespace foxglove_bridge

#include <rclcpp_components/register_node_macro.hpp>

// Register the component with class_loader.
// This acts as a sort of entry point, allowing the component to be discoverable when its library
// is being loaded into a running process.
RCLCPP_COMPONENTS_REGISTER_NODE(foxglove_bridge::FoxgloveBridge)
