#pragma once

#include <atomic>
#include <condition_variable>
#include <functional>
#include <memory>
#include <mutex>
#include <regex>
#include <string>
#include <thread>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include <ros/ros.h>

#include <foxglove_bridge_core/transport_manager.hpp>

namespace foxglove_bridge {

using ParamUpdateFunc = std::function<void(const ParameterList&)>;

/// ParameterBackend over the ROS 1 master parameter server.
///
/// Subscriptions are implemented by polling: the master's `subscribeParam`
/// push mechanism requires running a second XML-RPC endpoint (roscpp's own
/// already binds `paramUpdate` for its internal cache), which is not worth the
/// machinery for a first implementation.
/// TODO(ros1-bridge): consider push-based updates via a dedicated XmlRpcServer
/// (see the legacy ros-foxglove-bridge implementation).
class Ros1ParameterInterface : public ParameterBackend {
public:
  Ros1ParameterInterface(ros::NodeHandle nh, std::vector<std::regex> paramWhitelistPatterns,
                         std::chrono::milliseconds pollInterval);
  ~Ros1ParameterInterface() override;

  ParameterList getParams(const std::vector<std::string_view>& paramNames,
                          const std::chrono::duration<double>& timeout) override;
  void setParams(const ParameterList& params,
                 const std::chrono::duration<double>& timeout) override;
  void subscribeParams(const std::vector<std::string_view>& paramNames) override;
  void unsubscribeParams(const std::vector<std::string_view>& paramNames) override;

  void setParamUpdateCallback(ParamUpdateFunc paramUpdateFunc);

private:
  void pollSubscribedParams();

  ros::NodeHandle _nh;
  std::vector<std::regex> _paramWhitelistPatterns;
  std::chrono::milliseconds _pollInterval;

  std::mutex _mutex;
  ParamUpdateFunc _paramUpdateFunc;
  // Subscribed parameter names, with the last observed value (as XML) for
  // change detection.
  std::unordered_map<std::string, std::string> _subscribedParams;

  std::atomic<bool> _shuttingDown = false;
  std::unique_ptr<std::thread> _pollThread;
  std::mutex _pollMutex;
  std::condition_variable _pollCv;
};

}  // namespace foxglove_bridge
