#pragma once

#include <atomic>
#include <condition_variable>
#include <memory>
#include <mutex>
#include <optional>
#include <regex>
#include <string>
#include <thread>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include <ros/message_event.h>
#include <ros/ros.h>
#include <ros/subscribe_options.h>
#include <ros_babel_fish/generation/providers/integrated_description_provider.h>
#include <topic_tools/shape_shifter.h>

#include <foxglove/foxglove.hpp>
#include <foxglove_bridge_core/transport_manager.hpp>
#include <foxglove_bridge_core/types.hpp>
#include <foxglove_bridge_ros1/param_interface.hpp>

namespace foxglove_bridge {

class Ros1FoxgloveBridge : public BridgeDelegate {
public:
  using TopicAndDatatype = std::pair<std::string, std::string>;

  Ros1FoxgloveBridge(ros::NodeHandle nh, ros::NodeHandle privateNh);
  ~Ros1FoxgloveBridge() override;

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

  struct ClientAdvertisement {
    ros::Publisher publisher;
    std::string topicName;
    std::string topicType;
    std::string md5sum;
    std::string messageDefinition;
  };

  struct ServiceDetails {
    std::string type;
    std::string md5sum;
    // Keep the babel_fish descriptions alive for the schema data passed to the SDK.
    ros_babel_fish::ServiceDescription::ConstPtr description;
  };

  // One shared ROS subscription per channel, reference-counted by client subscriptions
  struct CachedLatchedMessage {
    std::vector<uint8_t> data;
    uint64_t timestamp;
  };
  struct ChannelSubscription {
    ros::Subscriber rosSubscription;
    std::unordered_set<ClientId> wsClientIds;
    std::unordered_set<ClientId> gatewayClientIds;
    // Last message per latched publisher (keyed by callerid), replayed to
    // late-joining clients. A latched ROS publisher only re-sends to new ROS
    // subscribers, and the bridge holds a single shared ROS subscription, so
    // late Foxglove clients would otherwise miss it. Lives and dies with the
    // ROS subscription: after the last client unsubscribes, the next
    // first-subscriber receives the latched message from ROS itself.
    std::map<std::string, CachedLatchedMessage> latchedMessages;
  };

  /// Master polling loop: topic/service discovery and connection graph
  /// updates, with exponential backoff (100ms doubling up to ~max_update_ms).
  void pollThread();

  void updateAdvertisedTopics(const std::vector<TopicAndDatatype>& topics);
  void updateAdvertisedServices(const std::vector<std::string>& serviceNames);

  void rosMessageHandler(ChannelId channelId,
                         const ros::MessageEvent<topic_tools::ShapeShifter const>& msgEvent);

  void handleServiceRequest(const foxglove::ServiceRequest& request,
                            foxglove::ServiceResponder&& responder);

  /// Look up a message description, serialized through a mutex: the provider
  /// is not thread-safe and is reached from both the poll thread and SDK
  /// callback threads.
  ros_babel_fish::MessageDescription::ConstPtr getMessageDescription(const std::string& datatype);
  ros_babel_fish::ServiceDescription::ConstPtr getServiceDescription(const std::string& type);

  ros::NodeHandle _nh;
  ros::NodeHandle _privateNh;

  // Created before (and therefore destroyed after) the TransportManager,
  // whose parameter worker calls into it.
  std::unique_ptr<Ros1ParameterInterface> _paramInterface;
  std::unique_ptr<TransportManager> _transports;

  ros_babel_fish::IntegratedDescriptionProvider _descriptionProvider;
  std::mutex _descriptionProviderMutex;

  std::unordered_map<ChannelId, foxglove::RawChannel> _channels;
  std::unordered_map<ChannelId, ChannelSubscription> _subscriptions;
  std::mutex _subscriptionsMutex;

  std::unordered_map<ClientChannelKey, ClientAdvertisement, ClientChannelKeyHash>
    _clientAdvertisedTopics;
  std::mutex _clientAdvertisementsMutex;

  std::unordered_map<std::string, ServiceDetails> _advertisedServices;
  std::unordered_map<std::string, std::unique_ptr<foxglove::ServiceHandler>> _serviceHandlers;
  std::mutex _servicesMutex;

  std::vector<std::regex> _topicWhitelistPatterns;
  std::vector<std::regex> _serviceWhitelistPatterns;
  std::vector<std::regex> _assetUriAllowlistPatterns;

  // Forwards /clock to clients (Time capability) when use_sim_time is set.
  ros::Subscriber _clockSubscription;

  std::atomic<int> _graphSubscriptionCount = 0;
  std::atomic<bool> _shuttingDown = false;
  std::unique_ptr<std::thread> _pollThread;
  std::mutex _pollMutex;
  std::condition_variable _pollCv;

  size_t _maxUpdatePeriodMs = 5000;
  int _serviceTypeRetrievalTimeoutMs = 250;
  int _subscriptionQueueLength = 10;
};

}  // namespace foxglove_bridge
