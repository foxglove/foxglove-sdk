#include <filesystem>
#include <type_traits>
#include <unordered_set>

#include <rclcpp/version.h>
#include <resource_retriever/retriever.hpp>

#include <foxglove_bridge/ros2_foxglove_bridge.hpp>
#include <foxglove_bridge/utils.hpp>
#include <foxglove_bridge/version.hpp>
#include <foxglove_bridge_core/capabilities.hpp>

namespace foxglove_bridge {
namespace {
inline bool isHiddenTopicOrService(const std::string& name) {
  if (name.empty()) {
    throw std::invalid_argument("Topic or service name can't be empty");
  }
  return name.front() == '_' || name.find("/_") != std::string::npos;
}

#if !RCLCPP_VERSION_GTE(28, 0, 0)
// Prior to Jazzy, GenericSubscription drops MessageInfo from the callback.
// This subclass overrides handle_serialized_message to forward it.
class GenericSubscriptionWithMessageInfo : public rclcpp::GenericSubscription {
public:
  using CallbackWithInfoT =
    std::function<void(std::shared_ptr<rclcpp::SerializedMessage>, const rclcpp::MessageInfo&)>;

  template <typename AllocatorT = std::allocator<void>>
  GenericSubscriptionWithMessageInfo(
    rclcpp::node_interfaces::NodeBaseInterface* node_base,
    const std::shared_ptr<rcpputils::SharedLibrary> ts_lib, const std::string& topic_name,
    const std::string& topic_type, const rclcpp::QoS& qos, CallbackWithInfoT callback,
    const rclcpp::SubscriptionOptionsWithAllocator<AllocatorT>& options)
      // The base class callback is never invoked because we override
      // handle_serialized_message below. A no-op is passed to satisfy
      // the constructor signature.
      : GenericSubscription(
          node_base, ts_lib, topic_name, topic_type, qos,
          [](std::shared_ptr<rclcpp::SerializedMessage>) {}, options)
      , callback_with_info_(std::move(callback)) {}

  void handle_serialized_message(const std::shared_ptr<rclcpp::SerializedMessage>& message,
                                 const rclcpp::MessageInfo& message_info) override {
    callback_with_info_(message, message_info);
  }

private:
  CallbackWithInfoT callback_with_info_;
};
#endif

}  // namespace

using namespace std::chrono_literals;
using namespace std::placeholders;

FoxgloveBridge::FoxgloveBridge(const rclcpp::NodeOptions& options)
    : Node("foxglove_bridge", options) {
  const char* rosDistro = std::getenv("ROS_DISTRO");
  RCLCPP_INFO(this->get_logger(), "Starting foxglove_bridge (%s, %s@%s)", rosDistro,
              foxglove_bridge::FOXGLOVE_BRIDGE_VERSION, foxglove_bridge::FOXGLOVE_BRIDGE_GIT_HASH);

  std::optional<std::map<std::string, std::string>> rosServerInfo;
  if (rosDistro && strlen(rosDistro) > 0) {
    rosServerInfo = std::map<std::string, std::string>{{"ROS_DISTRO", rosDistro}};
  }

  declareParameters(this);

  const auto port = static_cast<uint16_t>(
    std::clamp(this->get_parameter(PARAM_PORT).as_int(), int64_t{0}, int64_t{65535}));
  const auto address = this->get_parameter(PARAM_ADDRESS).as_string();
  _minQosDepth = saturatingToSizeT(this->get_parameter(PARAM_MIN_QOS_DEPTH).as_int());
  _maxQosDepth = saturatingToSizeT(this->get_parameter(PARAM_MAX_QOS_DEPTH).as_int());
  const bool useTls = this->get_parameter(PARAM_USETLS).as_bool();
  const std::string certfile = this->get_parameter(PARAM_CERTFILE).as_string();
  const std::string keyfile = this->get_parameter(PARAM_KEYFILE).as_string();
  const auto bestEffortQosTopicWhiteList =
    this->get_parameter(PARAM_BEST_EFFORT_QOS_TOPIC_WHITELIST).as_string_array();
  _bestEffortQosTopicWhiteListPatterns = parseRegexStrings(this, bestEffortQosTopicWhiteList);
  const auto topicWhiteList = this->get_parameter(PARAM_TOPIC_WHITELIST).as_string_array();
  _topicWhitelistPatterns = parseRegexStrings(this, topicWhiteList);
  const auto serviceWhiteList = this->get_parameter(PARAM_SERVICE_WHITELIST).as_string_array();
  _serviceWhitelistPatterns = parseRegexStrings(this, serviceWhiteList);
  const auto paramWhiteList = this->get_parameter(PARAM_PARAMETER_WHITELIST).as_string_array();
  const auto paramWhitelistPatterns = parseRegexStrings(this, paramWhiteList);
  _useSimTime = this->get_parameter("use_sim_time").as_bool();
  const auto capabilities = this->get_parameter(PARAM_CAPABILITIES).as_string_array();
  const auto clientTopicWhiteList =
    this->get_parameter(PARAM_CLIENT_TOPIC_WHITELIST).as_string_array();
  const auto clientTopicWhiteListPatterns = parseRegexStrings(this, clientTopicWhiteList);
  _includeHidden = this->get_parameter(PARAM_INCLUDE_HIDDEN).as_bool();
  const auto assetUriAllowlist = this->get_parameter(PARAM_ASSET_URI_ALLOWLIST).as_string_array();
  _assetUriAllowlistPatterns = parseRegexStrings(this, assetUriAllowlist);
  _disableLoanMessage = this->get_parameter(PARAM_DISABLE_LOAN_MESSAGE).as_bool();
  const auto ignoreUnresponsiveParamNodes =
    this->get_parameter(PARAM_IGN_UNRESPONSIVE_PARAM_NODES).as_bool();
  const bool publishClientCount = this->get_parameter(PARAM_PUBLISH_CLIENT_COUNT).as_bool();
  const auto messageBacklogSize =
    saturatingToSizeT(this->get_parameter(PARAM_MESSAGE_BACKLOG_SIZE).as_int());

  const bool debug = this->get_parameter(PARAM_DEBUG).as_bool();
  if (debug) {
    foxglove::setLogLevel(foxglove::LogLevel::Debug);
    this->get_logger().set_level(rclcpp::Logger::Level::Debug);
  }

  // Parameter backend, only when the Parameters capability is requested.
  const auto sdkCapabilities = processCapabilities(capabilities);
  if (hasCapability(sdkCapabilities, foxglove::WebSocketServerCapabilities::Parameters)) {
    _paramInterface = std::make_shared<ParameterInterface>(this, paramWhitelistPatterns,
                                                           ignoreUnresponsiveParamNodes
                                                             ? UnresponsiveNodePolicy::Ignore
                                                             : UnresponsiveNodePolicy::Retry);
    _paramInterface->setParamUpdateCallback(std::bind(&FoxgloveBridge::parameterUpdates, this, _1));
  }

  TransportOptions transportOptions;
  transportOptions.host = address;
  transportOptions.port = port;
  transportOptions.supportedEncodings = {"cdr", "json"};
  transportOptions.capabilities = capabilities;
  transportOptions.broadcastTimeCapability = _useSimTime;
  transportOptions.serverInfo = std::move(rosServerInfo);
  transportOptions.messageBacklogSize = messageBacklogSize;
  transportOptions.useTls = useTls;
  transportOptions.certfile = certfile;
  transportOptions.keyfile = keyfile;
  transportOptions.notifyClientCount = publishClientCount;
  transportOptions.remoteAccess = this->get_parameter(PARAM_REMOTE_ACCESS).as_bool();
  transportOptions.deviceToken = this->get_parameter(PARAM_DEVICE_TOKEN).as_string();
  transportOptions.foxgloveApiUrl = this->get_parameter(PARAM_FOXGLOVE_API_URL).as_string();
  transportOptions.sysinfo = this->get_parameter(PARAM_SYSINFO).as_bool();
  transportOptions.sysinfoTopic = this->get_parameter(PARAM_SYSINFO_TOPIC).as_string();
  transportOptions.sysinfoRefreshInterval =
    std::chrono::milliseconds(this->get_parameter(PARAM_SYSINFO_REFRESH_INTERVAL).as_int());

  Logger logger([this](BridgeLogLevel level, const std::string& message) {
    switch (level) {
      case BridgeLogLevel::Debug:
        RCLCPP_DEBUG(this->get_logger(), "%s", message.c_str());
        break;
      case BridgeLogLevel::Info:
        RCLCPP_INFO(this->get_logger(), "%s", message.c_str());
        break;
      case BridgeLogLevel::Warn:
        RCLCPP_WARN(this->get_logger(), "%s", message.c_str());
        break;
      case BridgeLogLevel::Error:
        RCLCPP_ERROR(this->get_logger(), "%s", message.c_str());
        break;
      case BridgeLogLevel::Fatal:
        RCLCPP_FATAL(this->get_logger(), "%s", message.c_str());
        break;
    }
  });

  _transports = std::make_unique<TransportManager>(std::move(transportOptions), *this,
                                                   _paramInterface.get(), std::move(logger));

  this->set_parameter(rclcpp::Parameter{PARAM_PORT, _transports->port()});
  RCLCPP_INFO(this->get_logger(), "Server listening on port %d", _transports->port());

  if (publishClientCount) {
    static const std::string CLIENT_COUNT_TOPIC = "/foxglove_bridge/client_count";
    _clientCountPublisher = this->create_publisher<std_msgs::msg::UInt32>(
      CLIENT_COUNT_TOPIC, rclcpp::QoS{rclcpp::KeepLast(1)}.transient_local());
    auto init_msg = std_msgs::msg::UInt32();
    init_msg.data = _transports->clientCount();
    _clientCountPublisher->publish(
      init_msg);  // Initialize transient local topic to current connection count
  }

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
        _transports->broadcastTime(static_cast<uint64_t>(timestamp));
      });
  }

  _rosgraphPollThread =
    std::make_unique<std::thread>(std::bind(&FoxgloveBridge::rosgraphPollThread, this));
}

FoxgloveBridge::~FoxgloveBridge() {
  _shuttingDown = true;
  RCLCPP_INFO(this->get_logger(), "Shutting down %s", this->get_name());
  if (_rosgraphPollThread) {
    _rosgraphPollThread->join();
  }
  // Stops the transports (gateway, then server) and the parameter worker.
  _transports->stop();
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
        if (_graphSubscriptionCount > 0) {
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

  // Collect channels to close outside the lock to avoid deadlock:
  // channel.close() can fire onUnsubscribe callbacks that re-acquire _subscriptionsMutex.
  std::vector<foxglove::RawChannel> channelsToClose;

  {
    std::lock_guard<std::mutex> lock(_subscriptionsMutex);

    // Remove channels for which the topic does not exist anymore
    for (auto channelIt = _channels.begin(); channelIt != _channels.end();) {
      auto& channel = channelIt->second;
      std::string schemaName = channel.schema().value().name;
      std::string topic(channel.topic());
      const TopicAndDatatype topicAndSchemaName = {topic, schemaName};
      if (latestTopics.find(topicAndSchemaName) == latestTopics.end()) {
        const auto channelId = channel.id();
        RCLCPP_INFO(this->get_logger(), "Removing channel %lu for topic \"%s\" (%s)", channelId,
                    topic.c_str(), schemaName.c_str());
        // Remove any active subscriptions for this channel
        _subscriptions.erase(channelId);
        channelsToClose.push_back(std::move(channel));
        channelIt = _channels.erase(channelIt);
      } else {
        channelIt++;
      }
    }

    // Advertise new topics
    for (const auto& topicAndDatatype : latestTopics) {
      const auto& topic = topicAndDatatype.first;
      const auto& schemaName = topicAndDatatype.second;

      if (std::find_if(_channels.begin(), _channels.end(), [&topic, &schemaName](const auto& kvp) {
            const auto& [channelId, channel] = kvp;
            return channel.topic() == topic && channel.schema().value().name == schemaName;
          }) != _channels.end()) {
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
          case foxglove_bridge::MessageDefinitionFormat::MSG:
            messageEncoding = "cdr";
            schema->encoding = "ros2msg";
            break;
          case foxglove_bridge::MessageDefinitionFormat::IDL:
            messageEncoding = "cdr";
            schema->encoding = "ros2idl";
            break;
          default:
            RCLCPP_WARN(this->get_logger(), "Unsupported message definition format for type %s",
                        schemaName.c_str());
            continue;
        }
      } catch (const foxglove_bridge::DefinitionNotFoundError& err) {
        // If the definition isn't found, advertise the channel with an empty schema as a fallback
        RCLCPP_WARN(this->get_logger(), "Could not find definition for type %s: %s",
                    schemaName.c_str(), err.what());
        schema = std::nullopt;
      } catch (const std::exception& err) {
        RCLCPP_ERROR(this->get_logger(),
                     "Failed to load schemaDefinition for topic \"%s\" (%s): %s", topic.c_str(),
                     schemaName.c_str(), err.what());
        continue;
      }

      // Create the new SDK channel
      auto channelResult =
        foxglove::RawChannel::create(topic, messageEncoding, schema, _transports->context());
      if (!channelResult.has_value()) {
        RCLCPP_ERROR(this->get_logger(), "Failed to create channel for topic \"%s\" (%s)",
                     topic.c_str(), foxglove::strerror(channelResult.error()));
        continue;
      }

      const ChannelId channelId = channelResult.value().id();
      RCLCPP_INFO(this->get_logger(), "Advertising new channel %lu for topic \"%s\"", channelId,
                  topic.c_str());
      _channels.insert({channelId, std::move(channelResult.value())});
    }
  }

  // Close channels after releasing _subscriptionsMutex, since close() may fire
  // onUnsubscribe callbacks that need to acquire _subscriptionsMutex.
  //
  // This is safe to do outside of the lock because this function is only called
  // single-threaded from rosgraphPollThread, and the removal of the channel
  // from _channels mean both createOrIncrementSubscriptionLocked and
  // rosMessageHandler gracefully ignore channels they can't find.
  for (auto& channel : channelsToClose) {
    channel.close();
  }
}

void FoxgloveBridge::updateAdvertisedServices() {
  if (!rclcpp::ok()) {
    return;
  } else if (!_transports->hasCapability(foxglove::WebSocketServerCapabilities::Services)) {
    return;
  }

  // Get the current list of visible services and datatypes from the ROS graph
  const auto serviceNamesAndTypes = this->get_node_graph_interface()->get_service_names_and_types();

  std::lock_guard<std::mutex> lock(_servicesMutex);

  // Remove advertisements for services that have been removed
  std::vector<std::string> servicesToRemove;
  for (const auto& [serviceName, _] : _advertisedServices) {
    if (serviceNamesAndTypes.find(serviceName) == serviceNamesAndTypes.end()) {
      servicesToRemove.push_back(serviceName);
    }
  }
  for (const auto& serviceName : servicesToRemove) {
    _advertisedServices.erase(serviceName);
    _serviceClients.erase(serviceName);
    _serviceHandlers.erase(serviceName);
    _transports->removeService(serviceName);
  }

  // Advertise new services
  for (const auto& serviceNamesAndType : serviceNamesAndTypes) {
    const auto& serviceName = serviceNamesAndType.first;
    const auto& datatypes = serviceNamesAndType.second;
    const auto& serviceType = datatypes.front();

    // Ignore the service if it's already advertised
    if (_advertisedServices.find(serviceName) != _advertisedServices.end()) {
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

    foxglove::ServiceSchema serviceSchema;
    serviceSchema.name = serviceType;

    // Read and initialize the service schema
    try {
      const auto requestTypeName = serviceType + foxglove_bridge::SERVICE_REQUEST_MESSAGE_SUFFIX;
      const auto responseTypeName = serviceType + foxglove_bridge::SERVICE_RESPONSE_MESSAGE_SUFFIX;
      const auto& [format, reqSchema] = _messageDefinitionCache.get_full_text(requestTypeName);
      const auto& resSchema = _messageDefinitionCache.get_full_text(responseTypeName).second;
      std::string schemaEncoding = "";
      std::string messageEncoding = "";
      switch (format) {
        case foxglove_bridge::MessageDefinitionFormat::MSG:
          schemaEncoding = "ros2msg";
          messageEncoding = "cdr";
          break;
        case foxglove_bridge::MessageDefinitionFormat::IDL:
          // REVIEW: Is this still true in the SDK?
          RCLCPP_WARN(this->get_logger(),
                      "IDL message definition format cannot be communicated over ws-protocol. "
                      "Service \"%s\" (%s) may not decode correctly in clients",
                      serviceName.c_str(), serviceType.c_str());
          schemaEncoding = "ros2idl";
          messageEncoding = "cdr";
          break;
        default:
          RCLCPP_ERROR(this->get_logger(), "Unsupported message definition format for type %s",
                       requestTypeName.c_str());
          continue;
      }
      serviceSchema.request = std::make_optional<foxglove::ServiceMessageSchema>();
      serviceSchema.request->encoding = messageEncoding;
      serviceSchema.request->schema = foxglove::Schema{
        requestTypeName,
        schemaEncoding,
        reinterpret_cast<const std::byte*>(reqSchema.data()),
        reqSchema.size(),
      };

      serviceSchema.response = std::make_optional<foxglove::ServiceMessageSchema>();
      serviceSchema.response->encoding = messageEncoding;
      serviceSchema.response->schema = foxglove::Schema{
        responseTypeName,
        schemaEncoding,
        reinterpret_cast<const std::byte*>(resSchema.data()),
        resSchema.size(),
      };
    } catch (const foxglove_bridge::DefinitionNotFoundError& err) {
      RCLCPP_WARN(this->get_logger(), "Could not find definition for type %s: %s",
                  serviceType.c_str(), err.what());
      // We still advertise the service, but with an empty schema
      serviceSchema.request = std::nullopt;
      serviceSchema.response = std::nullopt;
    } catch (const std::exception& err) {
      RCLCPP_WARN(this->get_logger(), "Failed to add service \"%s\" (%s): %s", serviceName.c_str(),
                  serviceType.c_str(), err.what());
      continue;
    }

    // Set up ROS service client
    try {
      auto clientOptions = rcl_client_get_default_options();
      auto [it, _] = _serviceClients.insert(
        {serviceName, std::make_shared<GenericClient>(this->get_node_base_interface().get(),
                                                      this->get_node_graph_interface(), serviceName,
                                                      serviceType, clientOptions)});
      this->get_node_services_interface()->add_client(it->second, _servicesCallbackGroup);
    } catch (const std::exception& ex) {
      RCLCPP_ERROR(this->get_logger(), "Failed to create service client for service %s: %s",
                   serviceName.c_str(), ex.what());
      continue;
    }

    auto handler = std::make_unique<foxglove::ServiceHandler>(
      [this](const foxglove::ServiceRequest& req, foxglove::ServiceResponder&& res) {
        this->handleServiceRequest(req, std::move(res));
      });

    _serviceHandlers.insert({serviceName, std::move(handler)});

    if (!_transports->addService(serviceName, serviceSchema, *_serviceHandlers.at(serviceName))) {
      continue;
    }

    _advertisedServices.insert({serviceName, serviceType});
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

  RCLCPP_INFO(this->get_logger(), "publishing connection graph");
  _transports->publishConnectionGraph(connectionGraph);
}

void FoxgloveBridge::onConnectionGraphSubscribe(bool subscribe) {
  RCLCPP_INFO(this->get_logger(), "received connection graph subscribe request");
  if (subscribe) {
    ++_graphSubscriptionCount;
    // TODO: This causes a deadlock in the SDK implementation
    // updateConnectionGraph(get_topic_names_and_types());
  } else if (_graphSubscriptionCount.fetch_sub(1) <= 0) {
    _graphSubscriptionCount.fetch_add(1);
  }
}

void FoxgloveBridge::onSubscribe(ChannelId channelId, ClientId clientId, bool isGateway,
                                 std::optional<SinkId> sinkId) {
  RCLCPP_INFO(this->get_logger(), "%sreceived subscribe request for channel %lu from client %u",
              isGateway ? "Gateway: " : "", channelId, clientId);
  createOrIncrementSubscription(channelId, clientId, isGateway, sinkId);
}

void FoxgloveBridge::onUnsubscribe(ChannelId channelId, ClientId clientId, bool isGateway) {
  RCLCPP_INFO(this->get_logger(), "%sreceived unsubscribe request for channel %lu from client %u",
              isGateway ? "Gateway: " : "", channelId, clientId);
  removeOrDecrementSubscription(channelId, clientId, isGateway);
}

Subscription FoxgloveBridge::createRosSubscription(ChannelId channelId, const std::string& topic,
                                                   const std::string& datatype,
                                                   const rclcpp::QoS& qos) {
  rclcpp::SubscriptionEventCallbacks eventCallbacks;
  eventCallbacks.incompatible_qos_callback =
    [this, topic, datatype](const rclcpp::QOSRequestedIncompatibleQoSInfo&) {
      RCLCPP_ERROR(this->get_logger(), "Incompatible subscriber QoS settings for topic \"%s\" (%s)",
                   topic.c_str(), datatype.c_str());
    };

  rclcpp::SubscriptionOptions subscriptionOptions;
  subscriptionOptions.event_callbacks = eventCallbacks;
  subscriptionOptions.callback_group = _subscriptionCallbackGroup;

#if RCLCPP_VERSION_GTE(28, 0, 0)
  return this->create_generic_subscription(
    topic, datatype, qos,
    [this, channelId](std::shared_ptr<const rclcpp::SerializedMessage> msg,
                      const rclcpp::MessageInfo& messageInfo) {
      this->rosMessageHandler(channelId, msg, messageInfo);
    },
    subscriptionOptions);
#else
  auto ts_lib = rclcpp::get_typesupport_library(datatype, "rosidl_typesupport_cpp");
  auto subscription = std::make_shared<GenericSubscriptionWithMessageInfo>(
    this->get_node_base_interface().get(), std::move(ts_lib), topic, datatype, qos,
    [this, channelId](std::shared_ptr<rclcpp::SerializedMessage> msg,
                      const rclcpp::MessageInfo& messageInfo) {
      this->rosMessageHandler(channelId, msg, messageInfo);
    },
    subscriptionOptions);
  this->get_node_topics_interface()->add_subscription(subscription,
                                                      subscriptionOptions.callback_group);
  return subscription;
#endif
}

void FoxgloveBridge::createOrIncrementSubscription(ChannelId channelId, ClientId clientId,
                                                   bool isGateway, std::optional<SinkId> sinkId) {
  std::lock_guard<std::mutex> lock(_subscriptionsMutex);
  createOrIncrementSubscriptionLocked(channelId, clientId, isGateway, sinkId);
}

void FoxgloveBridge::createOrIncrementSubscriptionLocked(ChannelId channelId, ClientId clientId,
                                                         bool isGateway,
                                                         std::optional<SinkId> sinkId) {
  auto channelIt = _channels.find(channelId);
  if (channelIt == _channels.end()) {
    RCLCPP_ERROR(this->get_logger(), "received subscribe request for unknown channel: %lu",
                 channelId);
    return;
  }

  auto& channel = channelIt->second;

  auto subIt = _subscriptions.find(channelId);
  bool isNewSubscription = (subIt == _subscriptions.end());

  if (isNewSubscription) {
    // First subscriber for this channel -- create the ROS subscription
    const std::string topic(channel.topic());
    const std::string datatype = channel.schema().value().name;
    const rclcpp::QoS qos = determineQoS(topic);

    ChannelSubscription channelSub;
    channelSub.rosSubscription = createRosSubscription(channelId, topic, datatype, qos);
    channelSub.qos = qos;

    if (qos.durability() == rclcpp::DurabilityPolicy::TransientLocal) {
      for (const auto& pub : this->get_publishers_info_by_topic(topic)) {
        Gid gid = pub.endpoint_gid();
        channelSub.publisherCaches[gid].maxMessages =
          std::max(static_cast<size_t>(1), pub.qos_profile().depth());
      }
    }

    auto [it, inserted] = _subscriptions.emplace(channelId, std::move(channelSub));
    subIt = it;

    RCLCPP_INFO(this->get_logger(), "Created ROS subscription on %s (%s) for channel %lu",
                topic.c_str(), datatype.c_str(), channelId);
  }

  // For transient_local topics, replay cached messages to the new client before adding
  // them to the broadcast set, so they don't miss latched values.
  if (!isNewSubscription && sinkId.has_value()) {
    for (const auto& [gid, cache] : subIt->second.publisherCaches) {
      for (const auto& cached : cache.messages) {
        channel.log(reinterpret_cast<const std::byte*>(cached.data.data()), cached.data.size(),
                    cached.timestamp, sinkId.value());
      }
    }
  }

  // Add client to the appropriate set
  if (isGateway) {
    subIt->second.gatewayClientIds.insert(clientId);
  } else {
    subIt->second.wsClientIds.insert(clientId);
  }
}

void FoxgloveBridge::removeOrDecrementSubscription(ChannelId channelId, ClientId clientId,
                                                   bool isGateway) {
  std::lock_guard<std::mutex> lock(_subscriptionsMutex);
  removeOrDecrementSubscriptionLocked(channelId, clientId, isGateway);
}

void FoxgloveBridge::removeOrDecrementSubscriptionLocked(ChannelId channelId, ClientId clientId,
                                                         bool isGateway) {
  auto subIt = _subscriptions.find(channelId);
  if (subIt == _subscriptions.end()) {
    RCLCPP_ERROR(this->get_logger(),
                 "Client %u tried unsubscribing from channel %lu but no subscription exists",
                 clientId, channelId);
    return;
  }

  // Remove client from the appropriate set
  if (isGateway) {
    subIt->second.gatewayClientIds.erase(clientId);
  } else {
    subIt->second.wsClientIds.erase(clientId);
  }

  // If no more subscribers, destroy the ROS subscription
  if (subIt->second.wsClientIds.empty() && subIt->second.gatewayClientIds.empty()) {
    RCLCPP_INFO(this->get_logger(),
                "Cleaned up ROS subscription for channel %lu (no more subscribers)", channelId);
    _subscriptions.erase(subIt);
  }
}

ClientAdvertisement FoxgloveBridge::createClientPublisher(const std::string& topicName,
                                                          const std::string& topicType,
                                                          const std::string& encoding,
                                                          const std::byte* schemaData,
                                                          size_t schemaLen) {
  // Create a JSON parser for this schema if needed
  std::shared_ptr<RosMsgParser::Parser> jsonParser;
  if (encoding == "json") {
    auto parserIt = _jsonParsers.find(topicType);
    if (parserIt != _jsonParsers.end()) {
      jsonParser = parserIt->second;
    } else {
      std::string schema;
      if (schemaLen > 0) {
        schema = std::string(reinterpret_cast<const char*>(schemaData), schemaLen);
      } else {
        auto [format, msgDefinition] = _messageDefinitionCache.get_full_text(topicType);
        if (format != foxglove_bridge::MessageDefinitionFormat::MSG) {
          throw std::runtime_error("Message definition (.msg) for schema " + topicType +
                                   " not found");
        }
        schema = msgDefinition;
      }
      jsonParser =
        std::make_shared<RosMsgParser::Parser>(topicName, RosMsgParser::ROSType(topicType), schema);
      _jsonParsers.insert({topicType, jsonParser});
    }
  }

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

  return ClientAdvertisement{std::move(publisher), topicName, topicType, encoding, jsonParser};
}

void FoxgloveBridge::publishClientData(const ClientAdvertisement& ad, const std::byte* data,
                                       size_t dataLen) {
  auto publishMessage = [&ad, this](const void* msgData, size_t size) {
    // Copy the message payload into a SerializedMessage object
    rclcpp::SerializedMessage serializedMessage{size};
    auto& rclSerializedMsg = serializedMessage.get_rcl_serialized_message();
    std::memcpy(rclSerializedMsg.buffer, msgData, size);
    rclSerializedMsg.buffer_length = size;
    // Publish the message
    if (_disableLoanMessage || !ad.publisher->can_loan_messages()) {
      ad.publisher->publish(serializedMessage);
    } else {
      ad.publisher->publish_as_loaned_msg(serializedMessage);
    }
  };

  if (ad.encoding == "cdr") {
    publishMessage(data, dataLen);
  } else if (ad.encoding == "json") {
    if (!ad.jsonParser) {
      throw std::runtime_error("no JSON parser found for schema \"" + ad.topicType + "\"");
    }
    thread_local RosMsgParser::ROS2_Serializer serializer;
    serializer.reset();
    const std::string jsonMessage(reinterpret_cast<const char*>(data), dataLen);
    ad.jsonParser->serializeFromJson(jsonMessage, &serializer);
    publishMessage(serializer.getBufferData(), serializer.getBufferSize());
  } else {
    throw std::runtime_error("unknown encoding \"" + ad.encoding + "\"");
  }
}

void FoxgloveBridge::onClientAdvertise(const ClientChannelInfo& channel, ClientId clientId,
                                       bool isGateway) {
  std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

  const ClientChannelKey key = {channel.id, clientId, isGateway};

  if (_clientAdvertisedTopics.find(key) != _clientAdvertisedTopics.end()) {
    throw ClientChannelError("Received client advertisement from client ID " +
                             std::to_string(clientId) + " for channel " +
                             std::to_string(channel.id) + " it had already advertised");
  }

  if (channel.schemaName.empty()) {
    throw ClientChannelError("Received client advertisement from client ID " +
                             std::to_string(clientId) + " for channel " +
                             std::to_string(channel.id) + " with empty schema name");
  }

  try {
    auto ad = createClientPublisher(channel.topic, channel.schemaName, channel.encoding,
                                    channel.schemaData, channel.schemaLen);
    RCLCPP_INFO(this->get_logger(),
                "%sClient ID %u is advertising \"%s\" (%s) on channel %lu with encoding \"%s\"",
                isGateway ? "Gateway: " : "", clientId, channel.topic.c_str(),
                channel.schemaName.c_str(), channel.id, channel.encoding.c_str());
    _clientAdvertisedTopics.emplace(key, std::move(ad));
  } catch (const std::exception& ex) {
    throw ClientChannelError("Failed to create publisher for client channel " +
                             std::to_string(channel.id) + ": " + ex.what());
  }
}

void FoxgloveBridge::onClientUnadvertise(ChannelId clientChannelId, ClientId clientId,
                                         bool isGateway) {
  std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

  const ClientChannelKey key = {clientChannelId, clientId, isGateway};

  auto it = _clientAdvertisedTopics.find(key);
  if (it == _clientAdvertisedTopics.end()) {
    throw ClientChannelError("Ignoring client unadvertisement from client ID " +
                             std::to_string(clientId) + " for unknown channel " +
                             std::to_string(clientChannelId));
  }

  const auto& publisher = it->second.publisher;
  RCLCPP_INFO(this->get_logger(),
              "%sClient ID %u is no longer advertising %s (%zu subscribers) on channel %lu",
              isGateway ? "Gateway: " : "", clientId, publisher->get_topic_name(),
              publisher->get_subscription_count(), clientChannelId);

  _clientAdvertisedTopics.erase(it);

  if (!_shuttingDown && rclcpp::ok()) {
    // Create a timer that immediately goes out of scope (so it never fires) which will trigger
    // the previously destroyed publisher to be cleaned up. This is a workaround for
    // https://github.com/ros2/rclcpp/issues/2146
    this->create_wall_timer(1s, []() {});
  }
}

void FoxgloveBridge::onClientMessage(ChannelId clientChannelId, ClientId clientId, bool isGateway,
                                     const std::byte* data, size_t dataLen) {
  ClientAdvertisement ad;
  {
    const ClientChannelKey key = {clientChannelId, clientId, isGateway};
    std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

    auto it = _clientAdvertisedTopics.find(key);
    if (it == _clientAdvertisedTopics.end()) {
      throw ClientChannelError("Dropping client message from client ID " +
                               std::to_string(clientId) + " for unknown channel " +
                               std::to_string(clientChannelId) +
                               ", client has no advertised topics");
    }

    ad = it->second;
  }

  try {
    publishClientData(ad, data, dataLen);
  } catch (const std::exception& ex) {
    throw ClientChannelError("Dropping client message on client channel " +
                             std::to_string(clientChannelId) + " from client ID " +
                             std::to_string(clientId) + ": " + ex.what());
  }
}

void FoxgloveBridge::parameterUpdates(const std::vector<foxglove::Parameter>& parameters) {
  _transports->publishParameterValues(parameters);
}

void FoxgloveBridge::rosMessageHandler(ChannelId channelId,
                                       std::shared_ptr<const rclcpp::SerializedMessage> msg,
                                       const rclcpp::MessageInfo& messageInfo) {
  // NOTE: Do not call any RCLCPP_* logging functions from this function. Otherwise, subscribing
  // to `/rosout` will cause a feedback loop
  const auto timestamp = this->now().nanoseconds();
  assert(timestamp >= 0 && "Timestamp is negative");
  const auto rclSerializedMsg = msg->get_rcl_serialized_message();

  std::lock_guard<std::mutex> lock(_subscriptionsMutex);
  auto channelIt = _channels.find(channelId);
  if (channelIt == _channels.end()) {
    return;
  }

  // Cache messages per-publisher for transient_local subscriptions so late subscribers receive
  // them.
  auto subIt = _subscriptions.find(channelId);
  if (subIt != _subscriptions.end() &&
      subIt->second.qos.durability() == rclcpp::DurabilityPolicy::TransientLocal) {
    Gid gid;
    const auto& rawGid = messageInfo.get_rmw_message_info().publisher_gid;
    std::copy(rawGid.data, rawGid.data + RMW_GID_STORAGE_SIZE, gid.begin());

    auto& pubCache = subIt->second.publisherCaches[gid];
    if (pubCache.messages.size() >= pubCache.maxMessages) {
      pubCache.messages.pop_front();
    }
    pubCache.messages.push_back(CachedMessage{
      {rclSerializedMsg.buffer, rclSerializedMsg.buffer + rclSerializedMsg.buffer_length},
      static_cast<uint64_t>(timestamp),
    });
  }

  // Log without sink_id to broadcast to all sinks (WebSocket server + Gateway).
  // Each sink internally handles routing to its subscribed clients.
  channelIt->second.log(reinterpret_cast<const std::byte*>(rclSerializedMsg.buffer),
                        rclSerializedMsg.buffer_length, timestamp);
}

void FoxgloveBridge::handleServiceRequest(const foxglove::ServiceRequest& request,
                                          foxglove::ServiceResponder&& responder) {
  RCLCPP_DEBUG(this->get_logger(), "Received a request for service %s",
               request.service_name.c_str());

  std::lock_guard<std::mutex> lock(_servicesMutex);
  auto serviceIt = _advertisedServices.find(request.service_name);
  if (serviceIt == _advertisedServices.end()) {
    std::string errorMessage = "Service " + request.service_name + " does not exist";
    RCLCPP_ERROR(this->get_logger(), "%s", errorMessage.c_str());
    std::move(responder).respondError(errorMessage);
    return;
  }

  const auto& [serviceName, serviceType] = *serviceIt;

  if (_serviceClients.find(serviceName) == _serviceClients.end()) {
    std::string errorMessage =
      "Service " + request.service_name + " is advertised but no client exists for it";
    RCLCPP_ERROR(this->get_logger(), "%s", errorMessage.c_str());
    std::move(responder).respondError(errorMessage);
    return;
  }

  auto client = _serviceClients.at(serviceName);
  if (!client->wait_for_service(1s)) {
    std::string errorMessage = "Service " + serviceName + " is not available";
    RCLCPP_ERROR(this->get_logger(), "%s", errorMessage.c_str());
    std::move(responder).respondError(errorMessage);
    return;
  }

  if (request.encoding != "cdr") {
    std::string errorMessage = "Service " + serviceName +
                               " received a request with an unsupported encoding " +
                               request.encoding;
    RCLCPP_ERROR(this->get_logger(), "%s", errorMessage.c_str());
    std::move(responder).respondError(errorMessage);
    return;
  }

  auto reqMessage = std::make_shared<rclcpp::SerializedMessage>(request.payload.size());
  std::memcpy(reqMessage->get_rcl_serialized_message().buffer, request.payload.data(),
              request.payload.size());
  reqMessage->get_rcl_serialized_message().buffer_length = request.payload.size();

  client->async_send_request(reqMessage, std::move(responder));
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

FoxgloveBridge::TopicQosInfo FoxgloveBridge::collectTopicQosInfo(const std::string& topic) {
  TopicQosInfo info;
  info.bestEffortForced = isWhitelisted(topic, _bestEffortQosTopicWhiteListPatterns);

  const auto publisherInfo = this->get_publishers_info_by_topic(topic);
  info.publisherCount = publisherInfo.size();
  for (const auto& publisher : publisherInfo) {
    const auto& qos = publisher.qos_profile();
    if (qos.reliability() == rclcpp::ReliabilityPolicy::Reliable) {
      ++info.reliableCount;
    }
    if (qos.durability() == rclcpp::DurabilityPolicy::TransientLocal) {
      ++info.transientLocalCount;
    }
    // Some RMWs do not retrieve history information of the publisher endpoint in which case the
    // history depth is 0. We use a lower limit of 1 here, so that the history depth is at least
    // equal to the number of publishers. This covers the case where there are multiple
    // transient_local publishers with a depth of 1 (e.g. multiple tf_static transform
    // broadcasters). See also
    // https://github.com/foxglove/ros-foxglove-bridge/issues/238 and
    // https://github.com/foxglove/ros-foxglove-bridge/issues/208
    info.totalHistoryDepth += std::max(static_cast<size_t>(1), qos.depth());
  }

  return info;
}

rclcpp::QoS FoxgloveBridge::determineQoS(const std::string& topic) {
  // Select an appropriate subscription QOS profile. This is similar to how ros2 topic echo
  // does it:
  // https://github.com/ros2/ros2cli/blob/619b3d1c9/ros2topic/ros2topic/verb/echo.py#L137-L194
  const auto info = collectTopicQosInfo(topic);

  size_t depth = std::max(info.totalHistoryDepth, _minQosDepth);
  if (depth > _maxQosDepth) {
    RCLCPP_WARN(this->get_logger(),
                "Limiting history depth for topic '%s' to %zu (was %zu). You may want to increase "
                "the max_qos_depth parameter value.",
                topic.c_str(), _maxQosDepth, depth);
    depth = _maxQosDepth;
  }

  rclcpp::QoS qos{rclcpp::KeepLast(depth)};

  // Force the QoS to be "best_effort" if in the whitelist
  if (info.bestEffortForced) {
    qos.best_effort();
  } else if (info.publisherCount > 0 && info.reliableCount == info.publisherCount) {
    // If all endpoints are reliable, ask for reliable
    qos.reliable();
  } else {
    if (info.reliableCount > 0) {
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
  if (info.publisherCount > 0 && info.transientLocalCount == info.publisherCount) {
    qos.transient_local();
  } else {
    if (info.transientLocalCount > 0) {
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

#ifdef FOXGLOVE_REMOTE_ACCESS
foxglove::QosProfile FoxgloveBridge::classifyRemoteAccessQos(
  const foxglove::ChannelDescriptor& channel) {
  // Mirror the reliability/durability decisions made by determineQoS: a topic qualifies for a
  // Reliable remote access profile only when it is not forced to best_effort by the whitelist
  // and every publisher offers both Reliable and TransientLocal. Anything else falls back to
  // the default (lossy data-track) profile.
  foxglove::QosProfile profile;
  const auto info = collectTopicQosInfo(std::string(channel.topic()));

  if (info.bestEffortForced || info.publisherCount == 0) {
    return profile;
  }

  const bool allReliable = info.reliableCount == info.publisherCount;
  const bool allTransientLocal = info.transientLocalCount == info.publisherCount;
  if (allReliable && allTransientLocal) {
    profile.reliability = foxglove::Reliability::Reliable;
  }
  return profile;
}

void FoxgloveBridge::onGatewayConnectionStatusChanged(
  foxglove::RemoteAccessConnectionStatus status) {
  const char* label = "unknown";
  switch (status) {
    case foxglove::RemoteAccessConnectionStatus::Connecting:
      label = "connecting";
      break;
    case foxglove::RemoteAccessConnectionStatus::Connected:
      label = "connected";
      break;
    case foxglove::RemoteAccessConnectionStatus::ShuttingDown:
      label = "shutting down";
      break;
    case foxglove::RemoteAccessConnectionStatus::Shutdown:
      label = "shutdown";
      break;
  }
  RCLCPP_INFO(this->get_logger(), "Remote access gateway status: %s", label);
}
#endif

void FoxgloveBridge::onClientConnect() {
  publishClientCount();
}

void FoxgloveBridge::onClientDisconnect() {
  publishClientCount();
}

void FoxgloveBridge::publishClientCount() {
  if (!_clientCountPublisher) {
    return;
  }
  const auto currentCount = _transports->clientCount();
  auto msg = std_msgs::msg::UInt32{};
  msg.data = currentCount;
  _clientCountPublisher->publish(msg);
}

}  // namespace foxglove_bridge

#include <rclcpp_components/register_node_macro.hpp>

// Register the component with class_loader.
// This acts as a sort of entry point, allowing the component to be discoverable when its library
// is being loaded into a running process.
RCLCPP_COMPONENTS_REGISTER_NODE(foxglove_bridge::FoxgloveBridge)
