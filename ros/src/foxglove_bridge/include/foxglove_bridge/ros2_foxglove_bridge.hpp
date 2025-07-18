#pragma once

#include <atomic>
#include <chrono>
#include <memory>
#include <regex>
#include <thread>

#include <rclcpp/rclcpp.hpp>
#include <rosgraph_msgs/msg/clock.hpp>
#include <rosx_introspection/ros_parser.hpp>
#include <websocketpp/common/connection_hdl.hpp>

#include <foxglove/foxglove.hpp>
#include <foxglove/server.hpp>
#include <foxglove/server/fetch_asset.hpp>
#include <foxglove_bridge/callback_queue.hpp>
#include <foxglove_bridge/foxglove_bridge.hpp>
#include <foxglove_bridge/generic_client.hpp>
#include <foxglove_bridge/message_definition_cache.hpp>
#include <foxglove_bridge/param_utils.hpp>
#include <foxglove_bridge/parameter_interface.hpp>
#include <foxglove_bridge/regex_utils.hpp>
#include <foxglove_bridge/server_factory.hpp>
#include <foxglove_bridge/utils.hpp>

namespace foxglove_bridge {

using ConnectionHandle = websocketpp::connection_hdl;
using LogLevel = foxglove_ws::WebSocketLogLevel;
using Subscription = rclcpp::GenericSubscription::SharedPtr;
using SubscriptionsByClient = std::map<ConnectionHandle, Subscription, std::owner_less<>>;
using Publication = rclcpp::GenericPublisher::SharedPtr;
using ClientPublications = std::unordered_map<foxglove_ws::ClientChannelId, Publication>;
using PublicationsByClient = std::map<ConnectionHandle, ClientPublications, std::owner_less<>>;

using SubscriptionCount = std::pair<Subscription, size_t>;
using MapOfSets = std::unordered_map<std::string, std::unordered_set<std::string>>;

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

  // BEGIN New SDK Components
  std::unique_ptr<foxglove::WebSocketServer> _sdkServer;
  std::unordered_map<ChannelId, foxglove::RawChannel> _sdkChannels;
  std::unordered_map<ChannelAndClientId, Subscription, PairHash> _sdkSubscriptions;
  std::unordered_map<ChannelAndClientId, ClientAdvertisement, PairHash> _clientAdvertisedTopics;
  // END New SDK Components

  std::unique_ptr<foxglove_ws::ServerInterface<ConnectionHandle>> _server;
  foxglove::MessageDefinitionCache _messageDefinitionCache;
  std::vector<std::regex> _topicWhitelistPatterns;
  std::vector<std::regex> _serviceWhitelistPatterns;
  std::vector<std::regex> _assetUriAllowlistPatterns;
  std::vector<std::regex> _bestEffortQosTopicWhiteListPatterns;
  std::shared_ptr<ParameterInterface> _paramInterface;
  std::unordered_map<foxglove_ws::ChannelId, foxglove_ws::ChannelWithoutId> _advertisedTopics;
  std::unordered_map<foxglove_ws::ServiceId, foxglove_ws::ServiceWithoutId> _advertisedServices;
  std::unordered_map<foxglove_ws::ChannelId, SubscriptionsByClient> _subscriptions;
  std::unordered_map<foxglove_ws::ServiceId, GenericClient::SharedPtr> _serviceClients;
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
  std::vector<std::string> _capabilities;
  std::atomic<bool> _subscribeGraphUpdates = false;
  bool _includeHidden = false;
  bool _disableLoanMessage = true;
  std::unique_ptr<foxglove_ws::CallbackQueue> _fetchAssetQueue;
  std::unordered_map<std::string, std::shared_ptr<RosMsgParser::Parser>> _jsonParsers;
  std::atomic<bool> _shuttingDown = false;

  void subscribeConnectionGraph(bool subscribe);

  void subscribe(ChannelId channelId, const foxglove::ClientMetadata& client);

  void unsubscribe(ChannelId channelId, const foxglove::ClientMetadata& client);

  void clientAdvertise(ClientId clientId, const foxglove::ClientChannel& channel);

  void clientUnadvertise(ClientId clientId, ChannelId clientChannelId);

  void clientMessage(ClientId clientId, ChannelId clientChannelId, const std::byte* data,
                     size_t dataLen);

  void setParameters(const std::vector<foxglove_ws::Parameter>& parameters,
                     const std::optional<std::string>& requestId, ConnectionHandle hdl);

  void getParameters(const std::vector<std::string>& parameters,
                     const std::optional<std::string>& requestId, ConnectionHandle hdl);

  void subscribeParameters(const std::vector<std::string>& parameters,
                           foxglove_ws::ParameterSubscriptionOperation op, ConnectionHandle);

  void parameterUpdates(const std::vector<foxglove_ws::Parameter>& parameters);

  void logHandler(LogLevel level, char const* msg);

  void rosMessageHandler(ChannelId channelId, SinkId sinkId,
                         std::shared_ptr<const rclcpp::SerializedMessage> msg);

  void serviceRequest(const foxglove_ws::ServiceRequest& request, ConnectionHandle clientHandle);

  void fetchAsset(const std::string_view uri, foxglove::FetchAssetResponder&& responder);

  bool hasCapability(const std::string& capability);

  rclcpp::QoS determineQoS(const std::string& topic);
};

}  // namespace foxglove_bridge
