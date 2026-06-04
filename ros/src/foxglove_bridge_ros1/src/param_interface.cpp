#include <map>

#include <ros/master.h>
#include <ros/names.h>
#include <xmlrpcpp/XmlRpcException.h>
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
                                               std::vector<std::regex> paramWhitelistPatterns)
    : _nh(std::move(nh))
    , _paramWhitelistPatterns(std::move(paramWhitelistPatterns)) {
  _xmlrpcServer.bind("paramUpdate", [this](XmlRpc::XmlRpcValue& params,
                                           XmlRpc::XmlRpcValue& result) {
    parameterUpdates(params, result);
  });
  _xmlrpcServer.start();
}

Ros1ParameterInterface::~Ros1ParameterInterface() {
  // Politely drop our registrations; the master would otherwise only clean
  // them up after a failed paramUpdate notification.
  std::unordered_set<std::string> subscribed;
  {
    std::lock_guard<std::mutex> lock(_mutex);
    subscribed.swap(_subscribedParams);
  }
  for (const auto& paramName : subscribed) {
    executeParamSubscription("unsubscribeParam", paramName);
  }
  _xmlrpcServer.shutdown();
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

bool Ros1ParameterInterface::executeParamSubscription(const std::string& opName,
                                                      const std::string& paramName) {
  // Registered under a distinct caller id so the master doesn't conflate these
  // subscriptions with roscpp's own (cached-parameter) registrations.
  XmlRpc::XmlRpcValue params, result, payload;
  params[0] = ros::this_node::getName() + "2";
  params[1] = _xmlrpcServer.getServerURI();
  params[2] = ros::names::resolve(paramName);

  if (ros::master::execute(opName, params, result, payload, false)) {
    ROS_DEBUG("%s '%s'", opName.c_str(), paramName.c_str());
    return true;
  }
  ROS_WARN("Failed to %s '%s': %s", opName.c_str(), paramName.c_str(), result.toXml().c_str());
  return false;
}

void Ros1ParameterInterface::subscribeParams(const std::vector<std::string_view>& paramNames) {
  for (const auto& nameView : paramNames) {
    const std::string name(nameView);
    if (!isWhitelisted(name, _paramWhitelistPatterns)) {
      ROS_ERROR("Parameter '%s' is not on the parameter whitelist", name.c_str());
      continue;
    }
    if (executeParamSubscription("subscribeParam", name)) {
      std::lock_guard<std::mutex> lock(_mutex);
      _subscribedParams.insert(name);
    }
  }
}

void Ros1ParameterInterface::unsubscribeParams(const std::vector<std::string_view>& paramNames) {
  for (const auto& nameView : paramNames) {
    const std::string name(nameView);
    if (executeParamSubscription("unsubscribeParam", name)) {
      std::lock_guard<std::mutex> lock(_mutex);
      _subscribedParams.erase(name);
    }
  }
}

void Ros1ParameterInterface::setParamUpdateCallback(ParamUpdateFunc paramUpdateFunc) {
  std::lock_guard<std::mutex> lock(_mutex);
  _paramUpdateFunc = std::move(paramUpdateFunc);
}

void Ros1ParameterInterface::parameterUpdates(XmlRpc::XmlRpcValue& params,
                                              XmlRpc::XmlRpcValue& result) {
  result[0] = 1;
  result[1] = std::string("");
  result[2] = 0;

  if (params.size() != 3) {
    ROS_ERROR("Parameter update called with invalid parameter size: %d", params.size());
    return;
  }

  try {
    const std::string paramName = ros::names::clean(params[1]);
    XmlRpc::XmlRpcValue paramValue = params[2];
    auto param = fromRosParam(paramName, paramValue);

    ParamUpdateFunc updateFunc;
    {
      std::lock_guard<std::mutex> lock(_mutex);
      updateFunc = _paramUpdateFunc;
    }
    if (updateFunc) {
      ParameterList update;
      update.push_back(std::move(param));
      updateFunc(update);
    }
  } catch (const std::exception& ex) {
    ROS_ERROR("Failed to update parameter: %s", ex.what());
  } catch (const XmlRpc::XmlRpcException& ex) {
    ROS_ERROR("Failed to update parameter: %s", ex.getMessage().c_str());
  } catch (...) {
    ROS_ERROR("Failed to update parameter");
  }
}

}  // namespace foxglove_bridge
