#include <map>

#include <xmlrpcpp/XmlRpcValue.h>

#include <foxglove_bridge_core/utils.hpp>
#include <foxglove_bridge_ros1/param_interface.hpp>

namespace foxglove_bridge {

namespace {

foxglove::ParameterValue valueFromRosParam(XmlRpc::XmlRpcValue& value) {
  using XmlRpc::XmlRpcValue;
  switch (value.getType()) {
    case XmlRpcValue::TypeBoolean:
      return foxglove::ParameterValue(static_cast<bool>(value));
    case XmlRpcValue::TypeInt:
      return foxglove::ParameterValue(static_cast<int64_t>(static_cast<int>(value)));
    case XmlRpcValue::TypeDouble:
      return foxglove::ParameterValue(static_cast<double>(value));
    case XmlRpcValue::TypeString:
      return foxglove::ParameterValue(static_cast<std::string&>(value));
    case XmlRpcValue::TypeArray: {
      std::vector<foxglove::ParameterValue> values;
      values.reserve(static_cast<size_t>(value.size()));
      for (int i = 0; i < value.size(); ++i) {
        values.push_back(valueFromRosParam(value[i]));
      }
      return foxglove::ParameterValue(std::move(values));
    }
    case XmlRpcValue::TypeStruct: {
      std::map<std::string, foxglove::ParameterValue> values;
      for (auto& [memberName, memberValue] : value) {
        values.insert({memberName, valueFromRosParam(memberValue)});
      }
      return foxglove::ParameterValue(std::move(values));
    }
    default:
      throw std::runtime_error("Unsupported parameter type " +
                               std::to_string(value.getType()));
  }
}

foxglove::Parameter fromRosParam(const std::string& name, XmlRpc::XmlRpcValue& value) {
  using XmlRpc::XmlRpcValue;
  switch (value.getType()) {
    case XmlRpcValue::TypeBoolean:
      return foxglove::Parameter(name, static_cast<bool>(value));
    case XmlRpcValue::TypeInt:
      return foxglove::Parameter(name, static_cast<int64_t>(static_cast<int>(value)));
    case XmlRpcValue::TypeDouble:
      return foxglove::Parameter(name, static_cast<double>(value));
    case XmlRpcValue::TypeString:
      return foxglove::Parameter(name, static_cast<std::string&>(value));
    default:
      return foxglove::Parameter(name, foxglove::ParameterType::None, valueFromRosParam(value));
  }
}

XmlRpc::XmlRpcValue toRosParam(const foxglove::ParameterValueView& value,
                               foxglove::ParameterType type) {
  using XmlRpc::XmlRpcValue;
  if (value.is<bool>()) {
    return XmlRpcValue(value.get<bool>());
  } else if (value.is<int64_t>()) {
    const auto intValue = value.get<int64_t>();
    if (type == foxglove::ParameterType::Float64) {
      // A whole-valued float round-trips as an integer with a type hint.
      return XmlRpcValue(static_cast<double>(intValue));
    }
    return XmlRpcValue(static_cast<int>(intValue));
  } else if (value.is<double>()) {
    return XmlRpcValue(value.get<double>());
  } else if (value.is<std::string>()) {
    return XmlRpcValue(value.get<std::string>());
  } else if (value.is<foxglove::ParameterValueView::Array>()) {
    XmlRpcValue arr;
    const auto values = value.get<foxglove::ParameterValueView::Array>();
    for (size_t i = 0; i < values.size(); ++i) {
      arr[static_cast<int>(i)] = toRosParam(values[i], foxglove::ParameterType::None);
    }
    return arr;
  } else if (value.is<foxglove::ParameterValueView::Dict>()) {
    XmlRpcValue obj;
    for (const auto& [memberName, memberValue] : value.get<foxglove::ParameterValueView::Dict>()) {
      obj[memberName] = toRosParam(memberValue, foxglove::ParameterType::None);
    }
    return obj;
  }
  throw std::runtime_error("Unsupported parameter value");
}

}  // namespace

Ros1ParameterInterface::Ros1ParameterInterface(ros::NodeHandle nh,
                                               std::vector<std::regex> paramWhitelistPatterns,
                                               std::chrono::milliseconds pollInterval)
    : _nh(std::move(nh))
    , _paramWhitelistPatterns(std::move(paramWhitelistPatterns))
    , _pollInterval(pollInterval) {
  _pollThread = std::make_unique<std::thread>([this]() {
    pollSubscribedParams();
  });
}

Ros1ParameterInterface::~Ros1ParameterInterface() {
  _shuttingDown = true;
  _pollCv.notify_all();
  if (_pollThread) {
    _pollThread->join();
  }
}

ParameterList Ros1ParameterInterface::getParams(const std::vector<std::string_view>& paramNames,
                                                const std::chrono::duration<double>& timeout) {
  (void)timeout;

  const bool allParametersRequested = paramNames.empty();
  std::vector<std::string> names;
  if (allParametersRequested) {
    if (!_nh.getParamNames(names)) {
      throw std::runtime_error("Failed to retrieve parameter names from the master");
    }
  } else {
    names.reserve(paramNames.size());
    for (const auto& name : paramNames) {
      names.emplace_back(name);
    }
  }

  ParameterList params;
  for (const auto& name : names) {
    if (!isWhitelisted(name, _paramWhitelistPatterns)) {
      if (!allParametersRequested) {
        ROS_ERROR("Parameter '%s' is not on the parameter whitelist", name.c_str());
      }
      continue;
    }

    try {
      XmlRpc::XmlRpcValue value;
      if (_nh.getParam(name, value)) {
        params.push_back(fromRosParam(name, value));
      } else if (!allParametersRequested) {
        ROS_WARN("Parameter '%s' is not set", name.c_str());
      }
    } catch (const std::exception& ex) {
      ROS_ERROR("Failed to read parameter '%s': %s", name.c_str(), ex.what());
    }
  }
  return params;
}

void Ros1ParameterInterface::setParams(const ParameterList& params,
                                       const std::chrono::duration<double>& timeout) {
  (void)timeout;

  for (const auto& param : params) {
    const std::string name(param.name());
    if (!isWhitelisted(name, _paramWhitelistPatterns)) {
      ROS_ERROR("Parameter '%s' is not on the parameter whitelist", name.c_str());
      continue;
    }

    try {
      const auto value = param.value();
      if (!value.has_value()) {
        _nh.deleteParam(name);
      } else {
        _nh.setParam(name, toRosParam(*value, param.type()));
      }
    } catch (const std::exception& ex) {
      ROS_ERROR("Failed to set parameter '%s': %s", name.c_str(), ex.what());
    }
  }
}

void Ros1ParameterInterface::subscribeParams(const std::vector<std::string_view>& paramNames) {
  std::lock_guard<std::mutex> lock(_mutex);
  for (const auto& nameView : paramNames) {
    const std::string name(nameView);
    if (!isWhitelisted(name, _paramWhitelistPatterns)) {
      ROS_ERROR("Parameter '%s' is not on the parameter whitelist", name.c_str());
      continue;
    }
    // Record the current value so only future changes are reported.
    std::string currentXml;
    try {
      XmlRpc::XmlRpcValue value;
      if (_nh.getParam(name, value)) {
        currentXml = value.toXml();
      }
    } catch (const std::exception& ex) {
      ROS_WARN("Failed to read parameter '%s': %s", name.c_str(), ex.what());
    }
    _subscribedParams.insert({name, currentXml});
    ROS_DEBUG("Subscribed to parameter '%s'", name.c_str());
  }
}

void Ros1ParameterInterface::unsubscribeParams(const std::vector<std::string_view>& paramNames) {
  std::lock_guard<std::mutex> lock(_mutex);
  for (const auto& nameView : paramNames) {
    _subscribedParams.erase(std::string(nameView));
  }
}

void Ros1ParameterInterface::setParamUpdateCallback(ParamUpdateFunc paramUpdateFunc) {
  std::lock_guard<std::mutex> lock(_mutex);
  _paramUpdateFunc = std::move(paramUpdateFunc);
}

void Ros1ParameterInterface::pollSubscribedParams() {
  while (!_shuttingDown) {
    {
      std::unique_lock<std::mutex> lock(_pollMutex);
      _pollCv.wait_for(lock, _pollInterval, [this] {
        return _shuttingDown.load();
      });
    }
    if (_shuttingDown) {
      return;
    }

    ParameterList updates;
    {
      std::lock_guard<std::mutex> lock(_mutex);
      if (!_paramUpdateFunc || _subscribedParams.empty()) {
        continue;
      }

      for (auto& [name, lastXml] : _subscribedParams) {
        try {
          XmlRpc::XmlRpcValue value;
          std::string currentXml;
          if (_nh.getParam(name, value)) {
            currentXml = value.toXml();
          }
          if (currentXml != lastXml) {
            lastXml = currentXml;
            updates.push_back(fromRosParam(name, value));
          }
        } catch (const std::exception& ex) {
          ROS_WARN_THROTTLE(10.0, "Failed to poll parameter '%s': %s", name.c_str(), ex.what());
        }
      }
    }

    if (!updates.empty()) {
      ParamUpdateFunc updateFunc;
      {
        std::lock_guard<std::mutex> lock(_mutex);
        updateFunc = _paramUpdateFunc;
      }
      if (updateFunc) {
        updateFunc(updates);
      }
    }
  }
}

}  // namespace foxglove_bridge
