#include <algorithm>
#include <cstring>
#include <unordered_set>

#include <ros/master.h>
#include <ros/serialization.h>
#include <ros/service.h>
#include <xmlrpcpp/XmlRpcValue.h>

#include <foxglove_bridge_core/capabilities.hpp>
#include <foxglove_bridge_core/utils.hpp>
#include <foxglove_bridge_ros1/generic_service.hpp>
#include <foxglove_bridge_ros1/ros1_foxglove_bridge.hpp>
#include <foxglove_bridge_ros1/service_utils.hpp>

namespace foxglove_bridge {

namespace {

constexpr char ROS1_MESSAGE_ENCODING[] = "ros1";
constexpr char ROS1_SCHEMA_ENCODING[] = "ros1msg";
constexpr size_t MIN_UPDATE_PERIOD_MS = 100;

std::vector<std::regex> parseRegexPatterns(const std::vector<std::string>& strings) {
  std::vector<std::regex> patterns;
  patterns.reserve(strings.size());
  for (const auto& pattern : strings) {
    try {
      patterns.emplace_back(pattern,
                            std::regex_constants::ECMAScript | std::regex_constants::icase);
    } catch (const std::exception& ex) {
      ROS_ERROR("Ignoring invalid regular expression '%s': %s", pattern.c_str(), ex.what());
    }
  }
  return patterns;
}

std::unordered_set<std::string> rpcValueToStringSet(const XmlRpc::XmlRpcValue& v) {
  std::unordered_set<std::string> set;
  for (int i = 0; i < v.size(); ++i) {
    set.insert(v[i]);
  }
  return set;
}

}  // namespace

Ros1FoxgloveBridge::Ros1FoxgloveBridge(ros::NodeHandle nh, ros::NodeHandle privateNh)
    : _nh(std::move(nh))
    , _privateNh(std::move(privateNh)) {
  const char* rosDistro = std::getenv("ROS_DISTRO");
  ROS_INFO("Starting foxglove_bridge (%s)", rosDistro ? rosDistro : "unknown");

  std::optional<std::map<std::string, std::string>> rosServerInfo;
  if (rosDistro && strlen(rosDistro) > 0) {
    rosServerInfo = std::map<std::string, std::string>{{"ROS_DISTRO", rosDistro}};
  }

  const int port = _privateNh.param<int>("port", 8765);
  const auto address = _privateNh.param<std::string>("address", "0.0.0.0");
  const bool useTls = _privateNh.param<bool>("tls", false);
  const auto certfile = _privateNh.param<std::string>("certfile", "");
  const auto keyfile = _privateNh.param<std::string>("keyfile", "");
  const auto topicWhitelist =
    _privateNh.param<std::vector<std::string>>("topic_whitelist", {".*"});
  _topicWhitelistPatterns = parseRegexPatterns(topicWhitelist);
  const auto serviceWhitelist =
    _privateNh.param<std::vector<std::string>>("service_whitelist", {".*"});
  _serviceWhitelistPatterns = parseRegexPatterns(serviceWhitelist);
  const auto paramWhitelist =
    _privateNh.param<std::vector<std::string>>("param_whitelist", {".*"});
  const auto capabilities = _privateNh.param<std::vector<std::string>>(
    "capabilities",
    {"clientPublish", "connectionGraph", "services", "parameters", "parametersSubscribe"});
  const int messageBacklogSize = _privateNh.param<int>("message_backlog_size", 1024);
  _maxUpdatePeriodMs =
    saturatingToSizeT(static_cast<int64_t>(_privateNh.param<int>("max_update_ms", 5000)));
  _serviceTypeRetrievalTimeoutMs =
    _privateNh.param<int>("service_type_retrieval_timeout_ms", 250);
  _subscriptionQueueLength = _privateNh.param<int>("subscription_queue_length", 10);

  const bool debug = _privateNh.param<bool>("debug", false);
  if (debug) {
    foxglove::setLogLevel(foxglove::LogLevel::Debug);
  }

  TransportOptions transportOptions;
  transportOptions.host = address;
  transportOptions.port = static_cast<uint16_t>(std::clamp(port, 0, 65535));
  transportOptions.supportedEncodings = {ROS1_MESSAGE_ENCODING};
  transportOptions.capabilities = capabilities;
  transportOptions.serverInfo = std::move(rosServerInfo);
  transportOptions.messageBacklogSize = saturatingToSizeT(messageBacklogSize);
  transportOptions.useTls = useTls;
  transportOptions.certfile = certfile;
  transportOptions.keyfile = keyfile;
  transportOptions.remoteAccess = _privateNh.param<bool>("remote_access", false);
  transportOptions.deviceToken = _privateNh.param<std::string>("device_token", "");
  transportOptions.foxgloveApiUrl = _privateNh.param<std::string>("foxglove_api_url", "");
  transportOptions.sysinfo = _privateNh.param<bool>("sysinfo", false);
  transportOptions.sysinfoTopic =
    _privateNh.param<std::string>("sysinfo_topic", "/foxglove_bridge/sysinfo");
  transportOptions.sysinfoRefreshInterval =
    std::chrono::milliseconds(_privateNh.param<int>("sysinfo_refresh_interval", 500));

  Logger logger([](BridgeLogLevel level, const std::string& message) {
    switch (level) {
      case BridgeLogLevel::Debug:
        ROS_DEBUG("%s", message.c_str());
        break;
      case BridgeLogLevel::Info:
        ROS_INFO("%s", message.c_str());
        break;
      case BridgeLogLevel::Warn:
        ROS_WARN("%s", message.c_str());
        break;
      case BridgeLogLevel::Error:
        ROS_ERROR("%s", message.c_str());
        break;
      case BridgeLogLevel::Fatal:
        ROS_FATAL("%s", message.c_str());
        break;
    }
  });

  // Parameter backend, only when the Parameters capability is requested.
  if (hasCapability(processCapabilities(capabilities),
                    foxglove::WebSocketServerCapabilities::Parameters)) {
    _paramInterface =
      std::make_unique<Ros1ParameterInterface>(_nh, parseRegexPatterns(paramWhitelist));
  }

  _transports = std::make_unique<TransportManager>(std::move(transportOptions), *this,
                                                   _paramInterface.get(), std::move(logger));

  if (_paramInterface) {
    _paramInterface->setParamUpdateCallback([this](const ParameterList& parameters) {
      _transports->publishParameterValues(parameters);
    });
  }

  ROS_INFO("Server listening on port %d", _transports->port());

  _pollThread = std::make_unique<std::thread>([this]() {
    pollThread();
  });
}

Ros1FoxgloveBridge::~Ros1FoxgloveBridge() {
  _shuttingDown = true;
  ROS_INFO("Shutting down foxglove_bridge");
  _pollCv.notify_all();
  if (_pollThread) {
    _pollThread->join();
  }
  _transports->stop();
  ROS_INFO("Shutdown complete");
}

void Ros1FoxgloveBridge::pollThread() {
  size_t updateCount = 0;

  while (!_shuttingDown && ros::ok()) {
    try {
      const bool servicesEnabled =
        _transports->hasCapability(foxglove::WebSocketServerCapabilities::Services);
      const bool querySystemState = servicesEnabled || _graphSubscriptionCount > 0;

      // Topics, via getTopicTypes.
      std::vector<TopicAndDatatype> topics;
      {
        XmlRpc::XmlRpcValue params, result, payload;
        params[0] = ros::this_node::getName();
        if (ros::master::execute("getTopicTypes", params, result, payload, false)) {
          topics.reserve(static_cast<size_t>(payload.size()));
          for (int i = 0; i < payload.size(); ++i) {
            topics.emplace_back(std::string(payload[i][0]), std::string(payload[i][1]));
          }
        } else {
          ROS_WARN("Failed to retrieve topics from ROS master");
        }
      }

      // Services and connection graph, via getSystemState.
      std::vector<std::string> serviceNames;
      foxglove::ConnectionGraph connectionGraph;
      if (querySystemState) {
        XmlRpc::XmlRpcValue params, result, payload;
        params[0] = ros::this_node::getName();
        if (ros::master::execute("getSystemState", params, result, payload, false) &&
            static_cast<int>(result[0]) == 1) {
          const auto& publishersXmlRpc = payload[0];
          const auto& subscribersXmlRpc = payload[1];
          const auto& servicesXmlRpc = payload[2];

          for (int i = 0; i < publishersXmlRpc.size(); ++i) {
            const std::string& name = publishersXmlRpc[i][0];
            if (isWhitelisted(name, _topicWhitelistPatterns)) {
              const auto nodes = rpcValueToStringSet(publishersXmlRpc[i][1]);
              connectionGraph.setPublishedTopic(name,
                                                std::vector<std::string>(nodes.begin(),
                                                                         nodes.end()));
            }
          }
          for (int i = 0; i < subscribersXmlRpc.size(); ++i) {
            const std::string& name = subscribersXmlRpc[i][0];
            if (isWhitelisted(name, _topicWhitelistPatterns)) {
              const auto nodes = rpcValueToStringSet(subscribersXmlRpc[i][1]);
              connectionGraph.setSubscribedTopic(name,
                                                 std::vector<std::string>(nodes.begin(),
                                                                          nodes.end()));
            }
          }
          for (int i = 0; i < servicesXmlRpc.size(); ++i) {
            const std::string& name = servicesXmlRpc[i][0];
            if (isWhitelisted(name, _serviceWhitelistPatterns)) {
              serviceNames.push_back(name);
              const auto nodes = rpcValueToStringSet(servicesXmlRpc[i][1]);
              connectionGraph.setAdvertisedService(name,
                                                   std::vector<std::string>(nodes.begin(),
                                                                            nodes.end()));
            }
          }
        } else {
          ROS_WARN("Failed to retrieve system state from ROS master");
        }
      }

      updateAdvertisedTopics(topics);
      if (servicesEnabled) {
        updateAdvertisedServices(serviceNames);
      }
      if (_graphSubscriptionCount > 0) {
        _transports->publishConnectionGraph(connectionGraph);
      }
    } catch (const std::exception& ex) {
      ROS_ERROR("Exception thrown in pollThread: %s", ex.what());
    }

    // Exponential backoff: 100ms -> 200ms -> 400ms ... up to max_update_ms.
    ++updateCount;
    const auto updatePeriodMs =
      std::max(MIN_UPDATE_PERIOD_MS,
               std::min(static_cast<size_t>(1) << updateCount, _maxUpdatePeriodMs));
    std::unique_lock<std::mutex> lock(_pollMutex);
    _pollCv.wait_for(lock, std::chrono::milliseconds(updatePeriodMs), [this] {
      return _shuttingDown.load();
    });
  }

  ROS_DEBUG("Master polling thread exiting");
}

void Ros1FoxgloveBridge::updateAdvertisedTopics(const std::vector<TopicAndDatatype>& topics) {
  std::unordered_set<TopicAndDatatype, PairHash> latestTopics;
  latestTopics.reserve(topics.size());
  for (const auto& topicAndDatatype : topics) {
    if (isWhitelisted(topicAndDatatype.first, _topicWhitelistPatterns)) {
      latestTopics.insert(topicAndDatatype);
    }
  }

  // Collect channels to close outside the lock to avoid deadlock:
  // channel.close() can fire onUnsubscribe callbacks that re-acquire _subscriptionsMutex.
  std::vector<foxglove::RawChannel> channelsToClose;

  {
    std::lock_guard<std::mutex> lock(_subscriptionsMutex);

    // Remove channels for which the topic does not exist anymore
    for (auto channelIt = _channels.begin(); channelIt != _channels.end();) {
      auto& channel = channelIt->second;
      const auto schema = channel.schema();
      const std::string schemaName = schema.has_value() ? schema->name : std::string();
      std::string topic(channel.topic());
      if (latestTopics.find({topic, schemaName}) == latestTopics.end()) {
        const auto channelId = channel.id();
        ROS_INFO("Removing channel %lu for topic \"%s\" (%s)", channelId, topic.c_str(),
                 schemaName.c_str());
        _subscriptions.erase(channelId);
        channelsToClose.push_back(std::move(channel));
        channelIt = _channels.erase(channelIt);
      } else {
        channelIt++;
      }
    }

    // Advertise new topics
    for (const auto& [topic, datatype] : latestTopics) {
      if (std::find_if(_channels.begin(), _channels.end(), [&](const auto& kvp) {
            const auto& channel = kvp.second;
            const auto schema = channel.schema();
            return channel.topic() == topic && schema.has_value() && schema->name == datatype;
          }) != _channels.end()) {
        continue;
      }

      std::optional<foxglove::Schema> schema = foxglove::Schema();
      schema->name = datatype;
      schema->encoding = ROS1_SCHEMA_ENCODING;

      // The description provider caches descriptions, so the schema data
      // remains valid for the lifetime of the provider.
      const auto msgDescription = getMessageDescription(datatype);
      if (msgDescription) {
        schema->data =
          reinterpret_cast<const std::byte*>(msgDescription->message_definition.data());
        schema->data_len = msgDescription->message_definition.size();
      } else {
        // Advertise the channel with an empty schema as a fallback.
        ROS_WARN("Could not find definition for type %s", datatype.c_str());
        schema = std::nullopt;
      }

      auto channelResult =
        foxglove::RawChannel::create(topic, ROS1_MESSAGE_ENCODING, schema,
                                     _transports->context());
      if (!channelResult.has_value()) {
        ROS_ERROR("Failed to create channel for topic \"%s\" (%s)", topic.c_str(),
                  foxglove::strerror(channelResult.error()));
        continue;
      }

      const ChannelId channelId = channelResult.value().id();
      ROS_INFO("Advertising new channel %lu for topic \"%s\" (%s)", channelId, topic.c_str(),
               datatype.c_str());
      _channels.insert({channelId, std::move(channelResult.value())});
    }
  }

  for (auto& channel : channelsToClose) {
    channel.close();
  }
}

void Ros1FoxgloveBridge::updateAdvertisedServices(const std::vector<std::string>& serviceNames) {
  std::lock_guard<std::mutex> lock(_servicesMutex);

  // Remove advertisements for services that have been removed
  std::vector<std::string> servicesToRemove;
  for (const auto& [serviceName, details] : _advertisedServices) {
    (void)details;
    if (std::find(serviceNames.begin(), serviceNames.end(), serviceName) == serviceNames.end()) {
      servicesToRemove.push_back(serviceName);
    }
  }
  for (const auto& serviceName : servicesToRemove) {
    _advertisedServices.erase(serviceName);
    _serviceHandlers.erase(serviceName);
    _transports->removeService(serviceName);
  }

  // Advertise new services
  for (const auto& serviceName : serviceNames) {
    if (_advertisedServices.find(serviceName) != _advertisedServices.end()) {
      continue;
    }

    ServiceDetails details;
    try {
      // The service type is not stored on the ROS master; probe the service
      // server's connection header for it.
      details.type = retrieveServiceType(
        serviceName, std::chrono::milliseconds(_serviceTypeRetrievalTimeoutMs));
      details.description = getServiceDescription(details.type);
    } catch (const std::exception& ex) {
      ROS_ERROR("Failed to retrieve type of service %s: %s", serviceName.c_str(), ex.what());
      continue;
    }

    foxglove::ServiceSchema serviceSchema;
    serviceSchema.name = details.type;

    std::string requestTypeName, responseTypeName;
    if (details.description) {
      details.md5sum = details.description->md5;

      requestTypeName = details.type + "Request";
      responseTypeName = details.type + "Response";

      serviceSchema.request = std::make_optional<foxglove::ServiceMessageSchema>();
      serviceSchema.request->encoding = ROS1_MESSAGE_ENCODING;
      serviceSchema.request->schema = foxglove::Schema{
        requestTypeName,
        ROS1_SCHEMA_ENCODING,
        reinterpret_cast<const std::byte*>(
          details.description->request->message_definition.data()),
        details.description->request->message_definition.size(),
      };

      serviceSchema.response = std::make_optional<foxglove::ServiceMessageSchema>();
      serviceSchema.response->encoding = ROS1_MESSAGE_ENCODING;
      serviceSchema.response->schema = foxglove::Schema{
        responseTypeName,
        ROS1_SCHEMA_ENCODING,
        reinterpret_cast<const std::byte*>(
          details.description->response->message_definition.data()),
        details.description->response->message_definition.size(),
      };
    } else {
      // We still advertise the service, but with an empty schema.
      ROS_WARN("Could not find definition for service type %s", details.type.c_str());
    }

    auto handler = std::make_unique<foxglove::ServiceHandler>(
      [this](const foxglove::ServiceRequest& req, foxglove::ServiceResponder&& res) {
        this->handleServiceRequest(req, std::move(res));
      });

    _serviceHandlers.insert({serviceName, std::move(handler)});

    if (!_transports->addService(serviceName, serviceSchema, *_serviceHandlers.at(serviceName))) {
      _serviceHandlers.erase(serviceName);
      continue;
    }

    ROS_INFO("Advertising service %s (%s)", serviceName.c_str(), details.type.c_str());
    _advertisedServices.insert({serviceName, std::move(details)});
  }
}

void Ros1FoxgloveBridge::onSubscribe(ChannelId channelId, ClientId clientId, bool isGateway,
                                     std::optional<SinkId> sinkId) {
  (void)sinkId;
  ROS_INFO("%sreceived subscribe request for channel %lu from client %u",
           isGateway ? "Gateway: " : "", channelId, clientId);

  std::lock_guard<std::mutex> lock(_subscriptionsMutex);

  auto channelIt = _channels.find(channelId);
  if (channelIt == _channels.end()) {
    ROS_ERROR("received subscribe request for unknown channel: %lu", channelId);
    return;
  }
  auto& channel = channelIt->second;

  auto subIt = _subscriptions.find(channelId);
  if (subIt == _subscriptions.end()) {
    // First subscriber for this channel -- create the ROS subscription.
    // Subscribe with the full MessageEvent so the connection header (which
    // carries the publisher's latching flag and callerid) is available.
    const std::string topic(channel.topic());
    boost::function<void(const ros::MessageEvent<topic_tools::ShapeShifter const>&)> callback =
      [this, channelId](const ros::MessageEvent<topic_tools::ShapeShifter const>& msgEvent) {
        this->rosMessageHandler(channelId, msgEvent);
      };

    ros::SubscribeOptions subscribeOptions;
    subscribeOptions.initByFullCallbackType<
      const ros::MessageEvent<topic_tools::ShapeShifter const>&>(
      topic, static_cast<uint32_t>(_subscriptionQueueLength), callback);

    ChannelSubscription channelSub;
    channelSub.rosSubscription = _nh.subscribe(subscribeOptions);

    auto [it, inserted] = _subscriptions.emplace(channelId, std::move(channelSub));
    (void)inserted;
    subIt = it;

    ROS_INFO("Created ROS subscription on %s for channel %lu", topic.c_str(), channelId);
  } else if (sinkId.has_value()) {
    // Replay latched messages to the late-joining client before adding it to
    // the broadcast set, so it doesn't miss latched values. (The client may
    // rarely receive a message twice if a broadcast lands between the SDK
    // accepting its subscription and this replay; latched topics carry
    // idempotent state, so duplicates are harmless.)
    for (const auto& [callerid, cached] : subIt->second.latchedMessages) {
      (void)callerid;
      channel.log(reinterpret_cast<const std::byte*>(cached.data.data()), cached.data.size(),
                  cached.timestamp, sinkId.value());
    }
  }

  if (isGateway) {
    subIt->second.gatewayClientIds.insert(clientId);
  } else {
    subIt->second.wsClientIds.insert(clientId);
  }
}

void Ros1FoxgloveBridge::onUnsubscribe(ChannelId channelId, ClientId clientId, bool isGateway) {
  ROS_INFO("%sreceived unsubscribe request for channel %lu from client %u",
           isGateway ? "Gateway: " : "", channelId, clientId);

  std::lock_guard<std::mutex> lock(_subscriptionsMutex);

  auto subIt = _subscriptions.find(channelId);
  if (subIt == _subscriptions.end()) {
    ROS_ERROR("Client %u tried unsubscribing from channel %lu but no subscription exists",
              clientId, channelId);
    return;
  }

  if (isGateway) {
    subIt->second.gatewayClientIds.erase(clientId);
  } else {
    subIt->second.wsClientIds.erase(clientId);
  }

  if (subIt->second.wsClientIds.empty() && subIt->second.gatewayClientIds.empty()) {
    ROS_INFO("Cleaned up ROS subscription for channel %lu (no more subscribers)", channelId);
    _subscriptions.erase(subIt);
  }
}

void Ros1FoxgloveBridge::rosMessageHandler(
  ChannelId channelId, const ros::MessageEvent<topic_tools::ShapeShifter const>& msgEvent) {
  // NOTE: Do not call any ROS_* logging functions from this function. Otherwise, subscribing
  // to `/rosout` will cause a feedback loop
  const auto timestamp = ros::Time::now().toNSec();
  const auto msg = msgEvent.getConstMessage();

  std::vector<uint8_t> buffer(msg->size());
  ros::serialization::OStream stream(buffer.data(), static_cast<uint32_t>(buffer.size()));
  msg->write(stream);

  std::lock_guard<std::mutex> lock(_subscriptionsMutex);
  auto channelIt = _channels.find(channelId);
  if (channelIt == _channels.end()) {
    return;
  }

  // Cache the last message per latched publisher for replay to late
  // subscribers.
  const auto connectionHeader = msgEvent.getConnectionHeaderPtr();
  if (connectionHeader) {
    const auto latchingIt = connectionHeader->find("latching");
    if (latchingIt != connectionHeader->end() && latchingIt->second == "1") {
      auto subIt = _subscriptions.find(channelId);
      if (subIt != _subscriptions.end()) {
        const auto calleridIt = connectionHeader->find("callerid");
        const std::string callerid =
          calleridIt != connectionHeader->end() ? calleridIt->second : std::string();
        subIt->second.latchedMessages[callerid] = CachedLatchedMessage{buffer, timestamp};
      }
    }
  }

  channelIt->second.log(reinterpret_cast<const std::byte*>(buffer.data()), buffer.size(),
                        timestamp);
}

void Ros1FoxgloveBridge::onClientAdvertise(const ClientChannelInfo& channel, ClientId clientId,
                                           bool isGateway) {
  if (channel.encoding != ROS1_MESSAGE_ENCODING) {
    throw ClientChannelError("Unsupported encoding \"" + channel.encoding +
                             "\" for client channel " + std::to_string(channel.id) +
                             " (expected \"" + ROS1_MESSAGE_ENCODING + "\")");
  }

  std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

  const ClientChannelKey key = {channel.id, clientId, isGateway};
  if (_clientAdvertisedTopics.find(key) != _clientAdvertisedTopics.end()) {
    throw ClientChannelError("Received client advertisement from client ID " +
                             std::to_string(clientId) + " for channel " +
                             std::to_string(channel.id) + " it had already advertised");
  }

  const auto msgDescription = getMessageDescription(channel.schemaName);
  if (!msgDescription) {
    throw ClientChannelError("Failed to retrieve type information for data type '" +
                             channel.schemaName + "'. Unable to advertise topic " + channel.topic);
  }

  ros::AdvertiseOptions advertiseOptions;
  advertiseOptions.datatype = channel.schemaName;
  advertiseOptions.has_header = false;
  advertiseOptions.latch = false;
  advertiseOptions.md5sum = msgDescription->md5;
  advertiseOptions.message_definition = msgDescription->message_definition;
  advertiseOptions.queue_size = static_cast<uint32_t>(_subscriptionQueueLength);
  advertiseOptions.topic = channel.topic;

  auto publisher = _nh.advertise(advertiseOptions);
  if (!publisher) {
    throw ClientChannelError("Failed to create publisher for topic " + channel.topic + " (" +
                             channel.schemaName + ")");
  }

  ROS_INFO("%sClient ID %u is advertising \"%s\" (%s) on channel %lu",
           isGateway ? "Gateway: " : "", clientId, channel.topic.c_str(),
           channel.schemaName.c_str(), channel.id);

  ClientAdvertisement ad;
  ad.publisher = std::move(publisher);
  ad.topicName = channel.topic;
  ad.topicType = channel.schemaName;
  ad.md5sum = msgDescription->md5;
  ad.messageDefinition = msgDescription->message_definition;
  _clientAdvertisedTopics.emplace(key, std::move(ad));

  // Wake the poll thread so other clients learn about the new topic promptly.
  _pollCv.notify_all();
}

void Ros1FoxgloveBridge::onClientUnadvertise(ChannelId clientChannelId, ClientId clientId,
                                             bool isGateway) {
  std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

  const ClientChannelKey key = {clientChannelId, clientId, isGateway};
  auto it = _clientAdvertisedTopics.find(key);
  if (it == _clientAdvertisedTopics.end()) {
    throw ClientChannelError("Ignoring client unadvertisement from client ID " +
                             std::to_string(clientId) + " for unknown channel " +
                             std::to_string(clientChannelId));
  }

  ROS_INFO("%sClient ID %u is no longer advertising %s on channel %lu",
           isGateway ? "Gateway: " : "", clientId, it->second.topicName.c_str(), clientChannelId);
  _clientAdvertisedTopics.erase(it);
}

void Ros1FoxgloveBridge::onClientMessage(ChannelId clientChannelId, ClientId clientId,
                                         bool isGateway, const std::byte* data, size_t dataLen) {
  topic_tools::ShapeShifter shapeShifter;
  {
    const ClientChannelKey key = {clientChannelId, clientId, isGateway};
    std::lock_guard<std::mutex> lock(_clientAdvertisementsMutex);

    auto it = _clientAdvertisedTopics.find(key);
    if (it == _clientAdvertisedTopics.end()) {
      throw ClientChannelError("Dropping client message from client ID " +
                               std::to_string(clientId) + " for unknown channel " +
                               std::to_string(clientChannelId));
    }
    const auto& ad = it->second;

    shapeShifter.morph(ad.md5sum, ad.topicType, ad.messageDefinition, "0");
    ros::serialization::IStream stream(
      const_cast<uint8_t*>(reinterpret_cast<const uint8_t*>(data)),
      static_cast<uint32_t>(dataLen));
    shapeShifter.read(stream);

    it->second.publisher.publish(shapeShifter);
  }
}

void Ros1FoxgloveBridge::onConnectionGraphSubscribe(bool subscribe) {
  ROS_INFO("received connection graph %s request", subscribe ? "subscribe" : "unsubscribe");
  if (subscribe) {
    ++_graphSubscriptionCount;
    _pollCv.notify_all();
  } else if (_graphSubscriptionCount.fetch_sub(1) <= 0) {
    _graphSubscriptionCount.fetch_add(1);
  }
}

void Ros1FoxgloveBridge::fetchAsset(std::string_view uri,
                                    foxglove::FetchAssetResponder&& responder) {
  // TODO(ros1-bridge): asset fetching via resource_retriever, gated behind the
  // "assets" capability (not in the default capability set yet).
  std::move(responder).respondError("Fetching assets is not supported: " + std::string(uri));
}

void Ros1FoxgloveBridge::handleServiceRequest(const foxglove::ServiceRequest& request,
                                              foxglove::ServiceResponder&& responder) {
  ROS_DEBUG("Received a request for service %s", request.service_name.c_str());

  ServiceDetails details;
  {
    std::lock_guard<std::mutex> lock(_servicesMutex);
    auto serviceIt = _advertisedServices.find(request.service_name);
    if (serviceIt == _advertisedServices.end()) {
      const std::string errorMessage = "Service " + request.service_name + " does not exist";
      ROS_ERROR("%s", errorMessage.c_str());
      std::move(responder).respondError(errorMessage);
      return;
    }
    details = serviceIt->second;
  }

  if (request.encoding != ROS1_MESSAGE_ENCODING) {
    const std::string errorMessage = "Service " + request.service_name +
                                     " received a request with an unsupported encoding " +
                                     request.encoding;
    ROS_ERROR("%s", errorMessage.c_str());
    std::move(responder).respondError(errorMessage);
    return;
  }

  if (details.md5sum.empty()) {
    const std::string errorMessage =
      "Type information for service " + request.service_name + " is unavailable";
    ROS_ERROR("%s", errorMessage.c_str());
    std::move(responder).respondError(errorMessage);
    return;
  }

  GenericService genReq, genRes;
  genReq.type = genRes.type = details.type;
  genReq.md5sum = genRes.md5sum = details.md5sum;
  genReq.data.resize(request.payload.size());
  std::memcpy(genReq.data.data(), request.payload.data(), request.payload.size());

  if (ros::service::call(request.service_name, genReq, genRes)) {
    std::move(responder).respondOk(reinterpret_cast<const std::byte*>(genRes.data.data()),
                                   genRes.data.size());
  } else {
    const std::string errorMessage =
      "Failed to call service " + request.service_name + " (" + details.type + ")";
    ROS_ERROR("%s", errorMessage.c_str());
    std::move(responder).respondError(errorMessage);
  }
}

ros_babel_fish::MessageDescription::ConstPtr Ros1FoxgloveBridge::getMessageDescription(
  const std::string& datatype) {
  std::lock_guard<std::mutex> lock(_descriptionProviderMutex);
  try {
    return _descriptionProvider.getMessageDescription(datatype);
  } catch (const std::exception& ex) {
    ROS_WARN("Failed to retrieve message description for %s: %s", datatype.c_str(), ex.what());
    return nullptr;
  }
}

ros_babel_fish::ServiceDescription::ConstPtr Ros1FoxgloveBridge::getServiceDescription(
  const std::string& type) {
  std::lock_guard<std::mutex> lock(_descriptionProviderMutex);
  try {
    return _descriptionProvider.getServiceDescription(type);
  } catch (const std::exception& ex) {
    ROS_WARN("Failed to retrieve service description for %s: %s", type.c_str(), ex.what());
    return nullptr;
  }
}

#ifdef FOXGLOVE_REMOTE_ACCESS
foxglove::QosProfile Ros1FoxgloveBridge::classifyRemoteAccessQos(
  const foxglove::ChannelDescriptor& channel) {
  // Latched topics carry idempotent state whose (replayed) messages must not
  // be dropped, so they ride the reliable control channel instead of a lossy
  // data track. ROS 1 only reveals latching via per-connection headers, so
  // this can only classify based on messages seen so far: a topic is treated
  // as latched once a latched publisher has been observed on it. Before the
  // first message arrives the default (lossy) profile applies.
  foxglove::QosProfile profile;
  std::lock_guard<std::mutex> lock(_subscriptionsMutex);
  auto subIt = _subscriptions.find(channel.id());
  if (subIt != _subscriptions.end() && !subIt->second.latchedMessages.empty()) {
    profile.reliability = foxglove::Reliability::Reliable;
  }
  return profile;
}

void Ros1FoxgloveBridge::onGatewayConnectionStatusChanged(
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
  ROS_INFO("Remote access gateway status: %s", label);
}
#endif

}  // namespace foxglove_bridge
