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
      : std::runtime_error(msg){};
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
  std::unordered_map<ChannelAndClientId, Subscription, PairHash> _subscriptions;
  std::unordered_map<ChannelAndClientId, ClientAdvertisement, PairHash> _clientAdvertisedTopics;
  foxglove::WebSocketServerCapabilities _capabilities;
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

  void rosMessageHandler(ChannelId channelId, SinkId sinkId,
                         std::shared_ptr<const rclcpp::SerializedMessage> msg);

  void handleServiceRequest(const foxglove::ServiceRequest& request,
                            foxglove::ServiceResponder&& responder);

  void fetchAsset(const std::string_view uri, foxglove::FetchAssetResponder&& responder);

  rclcpp::QoS determineQoS(const std::string& topic);
};

}  // namespace foxglove_bridge
