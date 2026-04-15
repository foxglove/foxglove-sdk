#pragma once

#include <atomic>
#include <chrono>
#include <memory>
#include <regex>
#include <thread>
#include <unordered_set>

#include <rclcpp/rclcpp.hpp>
#include <rosgraph_msgs/msg/clock.hpp>
#include <rosx_introspection/ros_parser.hpp>
#include <std_msgs/msg/u_int32.hpp>

#include <foxglove/fetch_asset.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/websocket.hpp>
#ifdef FOXGLOVE_REMOTE_ACCESS
#include <foxglove/remote_access.hpp>
#endif
#include <foxglove_bridge/generic_client.hpp>
#include <foxglove_bridge/message_definition_cache.hpp>
#include <foxglove_bridge/param_utils.hpp>
#include <foxglove_bridge/parameter_interface.hpp>
#include <foxglove_bridge/utils.hpp>

namespace foxglove_bridge {

extern const char FOXGLOVE_BRIDGE_VERSION[];
extern const char FOXGLOVE_BRIDGE_GIT_HASH[];

using Subscription = rclcpp::GenericSubscription::SharedPtr;
using Publication = rclcpp::GenericPublisher::SharedPtr;

using MapOfSets = std::unordered_map<std::string, std::unordered_set<std::string>>;
using ServicesByType = std::unordered_map<std::string, std::string>;

using ClientId = uint32_t;
using SinkId = uint64_t;
using ChannelId = uint64_t;
using ChannelAndClientId = std::pair<ChannelId, ClientId>;
struct ClientAdvertisement {
  Publication publisher;
  std::string topicName;
  std::string topicType;
  std::string encoding;
};

class ClientChannelError : public std::runtime_error {
public:
  ClientChannelError(const std::string& msg)
      : std::runtime_error(msg) {}
};

class FoxgloveBridge : public rclcpp::Node {
public:
  using TopicAndDatatype = std::pair<std::string, std::string>;

  FoxgloveBridge(const rclcpp::NodeOptions& options = rclcpp::NodeOptions());

  ~FoxgloveBridge();

  void rosgraphPollThread();

  void updateAdvertisedTopics(
    const std::map<std::string, std::vector<std::string>>& topicNamesAndTypes);

  void updateAdvertisedServices();

  void updateConnectionGraph(
    const std::map<std::string, std::vector<std::string>>& topicNamesAndTypes);

private:
  struct PairHash {
    template <class T1, class T2>
    std::size_t operator()(const std::pair<T1, T2>& pair) const {
      return std::hash<T1>()(pair.first) ^ std::hash<T2>()(pair.second);
    }
  };

  std::unique_ptr<foxglove::WebSocketServer> _server;
  std::unordered_map<ChannelId, foxglove::RawChannel> _channels;

  // One shared ROS subscription per channel, reference-counted by client subscriptions
  struct ChannelSubscription {
    Subscription rosSubscription;
    std::unordered_set<ClientId> wsClientIds;
    std::unordered_set<ClientId> gatewayClientIds;
    rclcpp::QoS qos{10};
    // Cached last message for transient_local topics, replayed to late subscribers
    std::vector<uint8_t> cachedMessageData;
    uint64_t cachedMessageTimestamp = 0;
    bool hasCachedMessage = false;
  };
  std::unordered_map<ChannelId, ChannelSubscription> _subscriptions;

  std::unordered_map<ChannelAndClientId, ClientAdvertisement, PairHash> _clientAdvertisedTopics;
  foxglove::WebSocketServerCapabilities _capabilities;

#ifdef FOXGLOVE_REMOTE_ACCESS
  std::unique_ptr<foxglove::RemoteAccessGateway> _gateway;
  std::unordered_map<ChannelAndClientId, ClientAdvertisement, PairHash>
    _gatewayClientAdvertisedTopics;
#endif
  ServicesByType _advertisedServices;
  std::unordered_map<std::string, GenericClient::SharedPtr> _serviceClients;
  std::unordered_map<std::string, std::unique_ptr<foxglove::ServiceHandler>> _serviceHandlers;

  foxglove_bridge::MessageDefinitionCache _messageDefinitionCache;
  std::vector<std::regex> _topicWhitelistPatterns;
  std::vector<std::regex> _serviceWhitelistPatterns;
  std::vector<std::regex> _assetUriAllowlistPatterns;
  std::vector<std::regex> _bestEffortQosTopicWhiteListPatterns;
  std::shared_ptr<ParameterInterface> _paramInterface;
  rclcpp::CallbackGroup::SharedPtr _subscriptionCallbackGroup;
  rclcpp::CallbackGroup::SharedPtr _clientPublishCallbackGroup;
  rclcpp::CallbackGroup::SharedPtr _servicesCallbackGroup;
  std::mutex _subscriptionsMutex;
  std::mutex _clientAdvertisementsMutex;
  std::mutex _servicesMutex;
  std::unique_ptr<std::thread> _rosgraphPollThread;
  size_t _minQosDepth = DEFAULT_MIN_QOS_DEPTH;
  size_t _maxQosDepth = DEFAULT_MAX_QOS_DEPTH;
  std::shared_ptr<rclcpp::Subscription<rosgraph_msgs::msg::Clock>> _clockSubscription;
  bool _useSimTime = false;
  std::atomic<bool> _subscribeGraphUpdates = false;
  bool _includeHidden = false;
  bool _disableLoanMessage = true;
  std::unordered_map<std::string, std::shared_ptr<RosMsgParser::Parser>> _jsonParsers;
  std::atomic<bool> _shuttingDown = false;
  foxglove::Context _serverContext;

  rclcpp::Publisher<std_msgs::msg::UInt32>::SharedPtr _clientCountPublisher;

  void subscribeConnectionGraph(bool subscribe);

  void subscribe(ChannelId channelId, const foxglove::ClientMetadata& client);

  void unsubscribe(ChannelId channelId, const foxglove::ClientMetadata& client);

  void clientAdvertise(ClientId clientId, const foxglove::ClientChannel& channel);

  void clientUnadvertise(ClientId clientId, ChannelId clientChannelId);

  void clientMessage(ClientId clientId, ChannelId clientChannelId, const std::byte* data,
                     size_t dataLen);

  std::vector<foxglove::Parameter> setParameters(
    const uint32_t clientId, const std::optional<std::string_view>& requestId,
    const std::vector<foxglove::ParameterView>& parameterViews);

  std::vector<foxglove::Parameter> getParameters(
    const uint32_t clientId, const std::optional<std::string_view>& requestId,
    const std::vector<std::string_view>& parameterNames);

  void subscribeParameters(const std::vector<std::string_view>& parameterNames);

  void unsubscribeParameters(const std::vector<std::string_view>& parameterNames);

  void parameterUpdates(const std::vector<foxglove::Parameter>& parameters);

  void rosMessageHandler(ChannelId channelId, std::shared_ptr<const rclcpp::SerializedMessage> msg);

  Subscription createRosSubscription(ChannelId channelId, const std::string& topic,
                                     const std::string& datatype, const rclcpp::QoS& qos);

  void createOrIncrementSubscription(ChannelId channelId, ClientId clientId, bool isGateway,
                                     std::optional<SinkId> sinkId = std::nullopt);
  void createOrIncrementSubscriptionLocked(ChannelId channelId, ClientId clientId, bool isGateway,
                                           std::optional<SinkId> sinkId = std::nullopt);

  void removeOrDecrementSubscription(ChannelId channelId, ClientId clientId, bool isGateway);
  void removeOrDecrementSubscriptionLocked(ChannelId channelId, ClientId clientId, bool isGateway);

  // Shared helpers for client publish (used by both WebSocket and gateway paths).
  // Must be called with _clientAdvertisementsMutex held. May throw.
  ClientAdvertisement createClientPublisher(const std::string& topicName,
                                            const std::string& topicType,
                                            const std::string& encoding,
                                            const std::byte* schemaData, size_t schemaLen);
  // Must NOT be called with _clientAdvertisementsMutex held. May throw.
  void publishClientData(const rclcpp::GenericPublisher::SharedPtr& publisher,
                         const std::string& encoding, const std::string& schemaName,
                         const std::byte* data, size_t dataLen);

  void handleServiceRequest(const foxglove::ServiceRequest& request,
                            foxglove::ServiceResponder&& responder);

  void fetchAsset(const std::string_view uri, foxglove::FetchAssetResponder&& responder);

  void onClientConnect();

  void onClientDisconnect();

  void publishClientCount();

  rclcpp::QoS determineQoS(const std::string& topic);

#ifdef FOXGLOVE_REMOTE_ACCESS
  void gatewaySubscribe(uint32_t clientId, const foxglove::ChannelDescriptor& channel);
  void gatewayUnsubscribe(uint32_t clientId, const foxglove::ChannelDescriptor& channel);
  void gatewayClientAdvertise(uint32_t clientId, const foxglove::ChannelDescriptor& channel);
  void gatewayClientUnadvertise(uint32_t clientId, const foxglove::ChannelDescriptor& channel);
  void gatewayClientMessage(uint32_t clientId, const foxglove::ChannelDescriptor& channel,
                            const std::byte* data, size_t dataLen);
  void gatewayConnectionStatusChanged(foxglove::RemoteAccessConnectionStatus status);
#endif
};

}  // namespace foxglove_bridge
