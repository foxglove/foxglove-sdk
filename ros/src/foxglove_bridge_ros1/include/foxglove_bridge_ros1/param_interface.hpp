#pragma once

#include <functional>
#include <mutex>
#include <regex>
#include <string>
#include <unordered_set>
#include <vector>

#include <ros/ros.h>
#include <ros/xmlrpc_manager.h>

#include <foxglove_bridge_core/transport_manager.hpp>

namespace foxglove_bridge {

using ParamUpdateFunc = std::function<void(const ParameterList&)>;

/// ParameterBackend over the ROS 1 master parameter server.
///
/// Subscriptions use the master's `subscribeParam` push mechanism: a second
/// ros::XMLRPCManager instance (roscpp's own already binds `paramUpdate` for
/// its internal cache) serves a `paramUpdate` endpoint that the master calls
/// when a subscribed parameter changes. Ported from the legacy
/// foxglove/ros-foxglove-bridge (MIT).
class Ros1ParameterInterface : public ParameterBackend {
public:
  Ros1ParameterInterface(ros::NodeHandle nh, std::vector<std::regex> paramWhitelistPatterns);
  ~Ros1ParameterInterface() override;

  ParameterList getParams(const std::vector<std::string_view>& paramNames,
                          const std::chrono::duration<double>& timeout) override;
  void setParams(const ParameterList& params,
                 const std::chrono::duration<double>& timeout) override;
  void subscribeParams(const std::vector<std::string_view>& paramNames) override;
  void unsubscribeParams(const std::vector<std::string_view>& paramNames) override;

  void setParamUpdateCallback(ParamUpdateFunc paramUpdateFunc);

private:
  /// `paramUpdate` XML-RPC endpoint, called by the master on parameter change.
  void parameterUpdates(XmlRpc::XmlRpcValue& params, XmlRpc::XmlRpcValue& result);

  /// Issue a master subscribeParam/unsubscribeParam call for one parameter.
  bool executeParamSubscription(const std::string& opName, const std::string& paramName);

  ros::NodeHandle _nh;
  std::vector<std::regex> _paramWhitelistPatterns;

  ros::XMLRPCManager _xmlrpcServer;

  std::mutex _mutex;
  ParamUpdateFunc _paramUpdateFunc;
  std::unordered_set<std::string> _subscribedParams;
};

}  // namespace foxglove_bridge
