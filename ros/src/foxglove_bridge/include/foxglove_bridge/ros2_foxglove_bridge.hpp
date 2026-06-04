#pragma once

#include <atomic>
#include <chrono>
#include <deque>
#include <map>
#include <memory>
#include <mutex>
#include <regex>
#include <thread>
#include <unordered_set>

#include <rclcpp/rclcpp.hpp>
#include <rmw/types.h>
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
#include <foxglove_bridge_core/transport_manager.hpp>
#include <foxglove_bridge_core/types.hpp>

namespace foxglove_bridge {

extern const char FOXGLOVE_BRIDGE_VERSION[];
extern const char FOXGLOVE_BRIDGE_GIT_HASH[];

using Subscription = rclcpp::GenericSubscription::SharedPtr;
using Publication = rclcpp::GenericPublisher::SharedPtr;

using ServicesByType = std::unordered_map<std::string, std::string>;

struct ClientAdvertisement {
  Publication publisher;
  std::string topicName;
  std::string topicType;
  std::string encoding;
  std::shared_ptr<RosMsgParser::Parser> jsonParser;
};

class FoxgloveBridge : public rclcpp::Node, public BridgeDelegate {
public:
  using TopicAndDatatype = std::pair<std::string, std::string>;

  FoxgloveBridge(const rclcpp::NodeOptions& options = rclcpp::NodeOptions());

  ~FoxgloveBridge() override;

  void rosgraphPollThread();

  void updateAdvertisedTopics(
    const std::map<std::string, std::vector<std::string>>& topicNamesAndTypes);

  void updateAdvertisedServices();

  void updateConnectionGraph(
    const std::map<std::string, std::vector<std::string>>& topicNamesAndTypes);

  /// Returns the current connection graph subscriber refcount. Exposed for testing.
  int graphSubscriptionCount() const noexcept {
    return _graphSubscriptionCount.load();
  }

  // BridgeDelegate (callbacks from the TransportManager, normalized across the
  // WebSocket server and the remote access gateway)
  void onSubscribe(ChannelId channelId, ClientId clientId, bool isGateway,
                   std::optional<SinkId> sinkId) override;
  void onUnsubscribe(ChannelId channelId, ClientId clientId, bool isGateway) override;
  void onClientAdvertise(const ClientChannelInfo& channel, ClientId clientId,
                         bool isGateway) override;
  void onClientUnadvertise(ChannelId clientChannelId, ClientId clientId, bool isGateway) override;
  void onClientMessage(ChannelId clientChannelId, ClientId clientId, bool isGateway,
                       const std::byte* data, size_t dataLen) override;
  void onConnectionGraphSubscribe(bool subscribe) override;
  void fetchAsset(std::string_view uri, foxglove::FetchAssetResponder&& responder) override;
  void onClientConnect() override;
  void onClientDisconnect() override;
#ifdef FOXGLOVE_REMOTE_ACCESS
  foxglove::QosProfile classifyRemoteAccessQos(
    const foxglove::ChannelDescriptor& channel) override;
  void onGatewayConnectionStatusChanged(foxglove::RemoteAccessConnectionStatus status) override;
#endif

private:
  // Client-advertised channels are keyed per transport: client IDs (and their
  // channel IDs) are only unique within a transport.
  struct ClientChannelKey {
    ChannelId channelId;
    ClientId clientId;
    bool isGateway;

    bool operator==(const ClientChannelKey& other) const {
      return channelId == other.channelId && clientId == other.clientId &&
             isGateway == other.isGateway;
    }
  };
  struct ClientChannelKeyHash {
    std::size_t operator()(const ClientChannelKey& key) const {
      return std::hash<ChannelId>()(key.channelId) ^ std::hash<ClientId>()(key.clientId) ^
             std::hash<bool>()(key.isGateway);
    }
  };

  std::unique_ptr<TransportManager> _transports;
  std::unordered_map<ChannelId, foxglove::RawChannel> _channels;

  // One shared ROS subscription per channel, reference-counted by client subscriptions
  struct CachedMessage {
    std::vector<uint8_t> data;
    uint64_t timestamp;
  };
  using Gid = std::array<uint8_t, RMW_GID_STORAGE_SIZE>;
  struct PublisherCache {
    std::deque<CachedMessage> messages;
    size_t maxMessages = 1;
  };
  struct ChannelSubscription {
    Subscription rosSubscription;
    std::unordered_set<ClientId> wsClientIds;
    std::unordered_set<ClientId> gatewayClientIds;
    rclcpp::QoS qos{10};
    // Per-publisher message cache for transient_local topics, replayed to late subscribers.
    std::map<Gid, PublisherCache> publisherCaches;
  };
  std::unordered_map<ChannelId, ChannelSubscription> _subscriptions;

  std::unordered_map<ClientChannelKey, ClientAdvertisement, ClientChannelKeyHash>
    _clientAdvertisedTopics;

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
  std::atomic<int> _graphSubscriptionCount = 0;
  bool _includeHidden = false;
  bool _disableLoanMessage = true;
  std::unordered_map<std::string, std::shared_ptr<RosMsgParser::Parser>> _jsonParsers;
  std::atomic<bool> _shuttingDown = false;

  rclcpp::Publisher<std_msgs::msg::UInt32>::SharedPtr _clientCountPublisher;

  void parameterUpdates(const std::vector<foxglove::Parameter>& parameters);

  void rosMessageHandler(ChannelId channelId, std::shared_ptr<const rclcpp::SerializedMessage> msg,
                         const rclcpp::MessageInfo& messageInfo);

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
  void publishClientData(const ClientAdvertisement& ad, const std::byte* data, size_t dataLen);

  void handleServiceRequest(const foxglove::ServiceRequest& request,
                            foxglove::ServiceResponder&& responder);

  void publishClientCount();

  struct TopicQosInfo {
    size_t publisherCount = 0;
    size_t reliableCount = 0;
    size_t transientLocalCount = 0;
    size_t totalHistoryDepth = 0;
    bool bestEffortForced = false;
  };

  TopicQosInfo collectTopicQosInfo(const std::string& topic);

  rclcpp::QoS determineQoS(const std::string& topic);
};

}  // namespace foxglove_bridge
