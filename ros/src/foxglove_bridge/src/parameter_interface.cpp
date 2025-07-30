#include "foxglove_bridge/parameter_interface.hpp"

#include <nlohmann/json.hpp>
#include <rclcpp/qos.hpp>
#include <rclcpp/version.h>

#include <foxglove_bridge/utils.hpp>

namespace {

constexpr char PARAM_SEP = '.';

#if RCLCPP_VERSION_MAJOR > 16
const rclcpp::ParametersQoS parameterQoS;
#else
const rmw_qos_profile_t& parameterQoS = rmw_qos_profile_parameters;
#endif

static std::pair<std::string, std::string> getNodeAndParamName(
  const std::string_view nodeNameAndParamName) {
  return {std::string(nodeNameAndParamName.substr(0UL, nodeNameAndParamName.find(PARAM_SEP))),
          std::string(nodeNameAndParamName.substr(nodeNameAndParamName.find(PARAM_SEP) + 1UL))};
}

static std::string prependNodeNameToParamName(const std::string& paramName,
                                              const std::string& nodeName) {
  return nodeName + PARAM_SEP + paramName;
}

static std::vector<std::byte> toByteArray(const std::vector<uint8_t>& arr) {
  std::vector<std::byte> result;
  result.reserve(arr.size());
  for (const auto& byte : arr) {
    result.emplace_back(std::byte{byte});
  }
  return result;
}

static std::vector<uint8_t> toUnsignedIntArray(const std::vector<std::byte>& arr) {
  std::vector<uint8_t> result;
  result.reserve(arr.size());
  for (const auto& byte : arr) {
    result.emplace_back(static_cast<uint8_t>(byte));
  }
  return result;
}

static rclcpp::Parameter toRosParam(const foxglove::Parameter& p) {
  using foxglove::Parameter;
  using foxglove::ParameterType;

  // Handle unset parameters
  if (!p.hasValue()) {
    return rclcpp::Parameter(std::string(p.name()));
  }

  // Handle primitive scalar types
  if (p.is<bool>()) {
    return rclcpp::Parameter(std::string(p.name()), p.get<bool>());
  } else if (p.is<double>() && p.type() == foxglove::ParameterType::Float64) {
    return rclcpp::Parameter(std::string(p.name()), p.get<double>());
  } else if (p.is<int64_t>()) {
    if (p.type() == foxglove::ParameterType::Float64) {
      // If the parameter is an integer, but the type flag is explicitly set to Float64, treat as a
      // double.
      return rclcpp::Parameter(std::string(p.name()), static_cast<double>(p.get<int64_t>()));
    }
    return rclcpp::Parameter(std::string(p.name()), p.get<int64_t>());
  } else if (p.is<std::string>()) {
    return rclcpp::Parameter(std::string(p.name()), p.get<std::string>());
  }

  // Handle arrays
  else if (p.isByteArray()) {
    const auto resultOrByteArray = p.getByteArray();
    if (!resultOrByteArray.has_value()) {
      std::string errorMessage = foxglove::strerror(resultOrByteArray.error());
      throw std::runtime_error("Failed to get byte array for parameter " + std::string(p.name()) +
                               ": " + errorMessage);
    }

    return rclcpp::Parameter(std::string(p.name()), toUnsignedIntArray(resultOrByteArray.value()));
  } else if (p.isArray<bool>()) {
    return rclcpp::Parameter(std::string(p.name()), p.getArray<bool>());
  } else if (p.isArray<int64_t>()) {
    if (p.type() == foxglove::ParameterType::Float64Array) {
      std::vector<double> doubleArray;
      for (const auto& value : p.getArray<int64_t>()) {
        doubleArray.emplace_back(static_cast<double>(value));
      }
      return rclcpp::Parameter(std::string(p.name()), std::move(doubleArray));
    }
    return rclcpp::Parameter(std::string(p.name()), p.getArray<int64_t>());
  } else if (p.isArray<double>()) {
    return rclcpp::Parameter(std::string(p.name()), p.getArray<double>());
  } else if (p.isArray<std::string>()) {
    return rclcpp::Parameter(std::string(p.name()), p.getArray<std::string>());
  } else {
    throw std::runtime_error("Unsupported parameter type");
  }

  return rclcpp::Parameter();
}

static foxglove::Parameter fromRosParam(const rclcpp::Parameter& p) {
  const auto type = p.get_type();

  if (type == rclcpp::ParameterType::PARAMETER_NOT_SET) {
    return foxglove::Parameter(p.get_name());
  } else if (type == rclcpp::ParameterType::PARAMETER_BOOL) {
    return foxglove::Parameter(p.get_name(), p.as_bool());
  } else if (type == rclcpp::ParameterType::PARAMETER_INTEGER) {
    foxglove::ParameterValue value(p.as_int());
    return foxglove::Parameter(p.get_name(), foxglove::ParameterType::None, std::move(value));
  } else if (type == rclcpp::ParameterType::PARAMETER_DOUBLE) {
    // All numerical values are serialized as doubles, so explicitly set the type flag to
    // foxglove::ParameterType::Float64 to indicate that this parameter should be interpreted as a
    // double.
    foxglove::ParameterValue value(p.as_double());
    foxglove::ParameterType foxgloveType = foxglove::ParameterType::Float64;
    return foxglove::Parameter(p.get_name(), foxgloveType, std::move(value));
  } else if (type == rclcpp::ParameterType::PARAMETER_STRING) {
    return foxglove::Parameter(p.get_name(), p.as_string());
  } else if (type == rclcpp::ParameterType::PARAMETER_BYTE_ARRAY) {
    return foxglove::Parameter(p.get_name(), toByteArray(p.as_byte_array()));
  }

  // Handle arrays
  else if (type == rclcpp::ParameterType::PARAMETER_BOOL_ARRAY) {
    foxglove::ParameterType foxgloveType = foxglove::ParameterType::None;
    std::vector<foxglove::ParameterValue> paramVec;
    for (const bool value : p.as_bool_array()) {
      paramVec.emplace_back(value);
    }

    return foxglove::Parameter(p.get_name(), foxgloveType,
                               foxglove::ParameterValue(std::move(paramVec)));
  } else if (type == rclcpp::ParameterType::PARAMETER_INTEGER_ARRAY) {
    return foxglove::Parameter(p.get_name(), p.as_integer_array());
  } else if (type == rclcpp::ParameterType::PARAMETER_DOUBLE_ARRAY) {
    return foxglove::Parameter(p.get_name(), p.as_double_array());
  } else if (type == rclcpp::ParameterType::PARAMETER_STRING_ARRAY) {
    std::vector<foxglove::ParameterValue> paramVec;
    foxglove::ParameterType paramType = foxglove::ParameterType::None;
    for (const std::string& value : p.as_string_array()) {
      paramVec.emplace_back(value);
    }
    return foxglove::Parameter(p.get_name(), paramType,
                               foxglove::ParameterValue(std::move(paramVec)));
  } else {
    throw std::runtime_error("Unsupported parameter type");
  }
}

}  // namespace

namespace foxglove_bridge {

using foxglove_bridge::isWhitelisted;

ParameterList ParameterInterface::cloneParameterList(const ParameterList& other) {
  ParameterList result;
  result.reserve(other.size());
  for (const foxglove::Parameter& param : other) {
    result.emplace_back(param.clone());
  }
  return result;
}

ParameterInterface::ParameterInterface(rclcpp::Node* node,
                                       std::vector<std::regex> paramWhitelistPatterns,
                                       UnresponsiveNodePolicy unresponsiveNodePolicy)
    : _node(node)
    , _paramWhitelistPatterns(paramWhitelistPatterns)
    , _callbackGroup(node->create_callback_group(rclcpp::CallbackGroupType::Reentrant))
    , _ignoredNodeNames({node->get_fully_qualified_name()})
    , _unresponsiveNodePolicy(unresponsiveNodePolicy) {}

ParameterList ParameterInterface::getParams(const std::vector<std::string_view>& paramNames,
                                            const std::chrono::duration<double>& timeout) {
  std::lock_guard<std::mutex> lock(_mutex);

  std::unordered_map<std::string, std::vector<std::string>> paramNamesByNodeName;
  const auto thisNode = _node->get_fully_qualified_name();

  if (!paramNames.empty()) {
    // Break apart fully qualified {node_name}.{param_name} strings and build a
    // mape of node names to the list of parameters for each node
    for (const auto& fullParamName : paramNames) {
      const auto& [nodeName, paramName] = getNodeAndParamName(fullParamName);
      paramNamesByNodeName[nodeName].push_back(paramName);
    }

    RCLCPP_DEBUG(_node->get_logger(), "Getting %zu parameters from %zu nodes...", paramNames.size(),
                 paramNamesByNodeName.size());
  } else {
    // Make a map of node names to empty parameter lists
    // Only consider nodes that offer services to list & get parameters.
    for (const auto& fqnNodeName : _node->get_node_names()) {
      if (_ignoredNodeNames.find(fqnNodeName) != _ignoredNodeNames.end()) {
        continue;
      }
      const auto [nodeNs, nodeName] = getNodeAndNodeNamespace(fqnNodeName);
      const auto serviceNamesAndTypes =
        _node->get_service_names_and_types_by_node(nodeName, nodeNs);

      bool listParamsSrvFound = false, getParamsSrvFound = false;
      for (const auto& [serviceName, serviceTypes] : serviceNamesAndTypes) {
        constexpr char GET_PARAMS_SERVICE_TYPE[] = "rcl_interfaces/srv/GetParameters";
        constexpr char LIST_PARAMS_SERVICE_TYPE[] = "rcl_interfaces/srv/ListParameters";

        if (!getParamsSrvFound) {
          getParamsSrvFound = std::find(serviceTypes.begin(), serviceTypes.end(),
                                        GET_PARAMS_SERVICE_TYPE) != serviceTypes.end();
        }
        if (!listParamsSrvFound) {
          listParamsSrvFound = std::find(serviceTypes.begin(), serviceTypes.end(),
                                         LIST_PARAMS_SERVICE_TYPE) != serviceTypes.end();
        }
      }

      if (listParamsSrvFound && getParamsSrvFound) {
        paramNamesByNodeName.insert({fqnNodeName, {}});
      }
    }

    if (!paramNamesByNodeName.empty()) {
      RCLCPP_DEBUG(_node->get_logger(), "Getting all parameters from %zu nodes...",
                   paramNamesByNodeName.size());
    }
  }

  std::unordered_map<std::string, std::future<ParameterList>> getParametersFuture;
  for (const auto& [nodeName, nodeParamNames] : paramNamesByNodeName) {
    if (nodeName == thisNode) {
      continue;
    }

    auto paramClientIt = _paramClientsByNode.find(nodeName);
    if (paramClientIt == _paramClientsByNode.end()) {
      const auto insertedPair = _paramClientsByNode.emplace(
        nodeName,
        rclcpp::AsyncParametersClient::make_shared(_node, nodeName, parameterQoS, _callbackGroup));
      paramClientIt = insertedPair.first;
    }

    getParametersFuture.emplace(
      nodeName, std::async(std::launch::async, &ParameterInterface::getNodeParameters, this,
                           paramClientIt->second, nodeName, nodeParamNames, timeout));
  }

  ParameterList result;
  for (auto& [nodeName, future] : getParametersFuture) {
    try {
      // Move the parameters from the future into result. Must be done using rvalue reference
      // to avoid copying the parameters.
      for (auto& param : future.get()) {
        result.push_back(param.clone());
      }
    } catch (const std::exception& e) {
      RCLCPP_ERROR(_node->get_logger(), "Failed to retrieve parameters from node '%s': %s",
                   nodeName.c_str(), e.what());

      if (_unresponsiveNodePolicy == UnresponsiveNodePolicy::Ignore) {
        // Certain nodes may fail to handle incoming service requests — for example, if they're
        // stuck in a busy loop or otherwise unresponsive. In such cases, attempting to retrieve
        // parameter names or values can result in timeouts. To avoid repeated failures, these nodes
        // are added to an ignore list, and future parameter-related service calls to them will be
        // skipped.
        _ignoredNodeNames.insert(nodeName);
        RCLCPP_WARN(_node->get_logger(),
                    "Adding node %s to the ignore list to prevent repeated timeouts or failures in "
                    "future parameter requests.",
                    nodeName.c_str());
      }
    }
  }

  return result;
}

void ParameterInterface::setParams(const ParameterList& parameters,
                                   const std::chrono::duration<double>& timeout) {
  std::lock_guard<std::mutex> lock(_mutex);

  rclcpp::ParameterMap paramsByNode;
  for (const auto& param : parameters) {
    if (!isWhitelisted(std::string(param.name()), _paramWhitelistPatterns)) {
      return;
    }

    const auto rosParam = toRosParam(param);
    const auto& [nodeName, paramName] = getNodeAndParamName(rosParam.get_name());
    paramsByNode[nodeName].emplace_back(paramName, rosParam.get_parameter_value());
  }

  std::vector<std::future<void>> setParametersFuture;
  for (const auto& [nodeName, params] : paramsByNode) {
    auto paramClientIt = _paramClientsByNode.find(nodeName);
    if (paramClientIt == _paramClientsByNode.end()) {
      const auto insertedPair = _paramClientsByNode.emplace(
        nodeName,
        rclcpp::AsyncParametersClient::make_shared(_node, nodeName, parameterQoS, _callbackGroup));
      paramClientIt = insertedPair.first;
    }

    setParametersFuture.emplace_back(std::async(std::launch::async,
                                                &ParameterInterface::setNodeParameters, this,
                                                paramClientIt->second, nodeName, params, timeout));
  }

  for (auto& future : setParametersFuture) {
    try {
      future.get();
    } catch (const std::exception& e) {
      RCLCPP_ERROR(_node->get_logger(), "Exception when setting parameters: %s", e.what());
    }
  }
}

void ParameterInterface::subscribeParams(const std::vector<std::string_view>& paramNames) {
  std::lock_guard<std::mutex> lock(_mutex);

  std::unordered_set<std::string> nodesToSubscribe;
  for (const auto& paramName : paramNames) {
    if (!isWhitelisted(std::string(paramName), _paramWhitelistPatterns)) {
      return;
    }

    const auto& [nodeName, paramN] = getNodeAndParamName(paramName);
    auto [subscribedParamsit, wasNewlyCreated] = _subscribedParamsByNode.try_emplace(nodeName);

    auto& subscribedNodeParams = subscribedParamsit->second;
    subscribedNodeParams.insert(paramN);

    if (wasNewlyCreated) {
      nodesToSubscribe.insert(nodeName);
    }
  }

  for (const auto& nodeName : nodesToSubscribe) {
    auto paramClientIt = _paramClientsByNode.find(nodeName);
    if (paramClientIt == _paramClientsByNode.end()) {
      const auto insertedPair = _paramClientsByNode.emplace(
        nodeName,
        rclcpp::AsyncParametersClient::make_shared(_node, nodeName, parameterQoS, _callbackGroup));
      paramClientIt = insertedPair.first;
    }

    auto& paramClient = paramClientIt->second;

    _paramSubscriptionsByNode[nodeName] = paramClient->on_parameter_event(
      [this, nodeName](rcl_interfaces::msg::ParameterEvent::ConstSharedPtr msg) {
        RCLCPP_DEBUG(_node->get_logger(), "Retrieved param update for node %s: %zu params changed",
                     nodeName.c_str(), msg->changed_parameters.size());

        ParameterList result;
        const auto& subscribedNodeParams = _subscribedParamsByNode[nodeName];
        for (const auto& param : msg->changed_parameters) {
          if (subscribedNodeParams.find(param.name) != subscribedNodeParams.end()) {
            result.push_back(fromRosParam(
              rclcpp::Parameter(prependNodeNameToParamName(param.name, nodeName), param.value)));
          }
        }

        if (!result.empty() && _paramUpdateFunc) {
          _paramUpdateFunc(result);
        }
      });
  }
}

void ParameterInterface::unsubscribeParams(const std::vector<std::string_view>& paramNames) {
  std::lock_guard<std::mutex> lock(_mutex);

  for (const auto& paramName : paramNames) {
    const auto& [nodeName, paramN] = getNodeAndParamName(paramName);

    const auto subscribedNodeParamsIt = _subscribedParamsByNode.find(nodeName);
    if (subscribedNodeParamsIt != _subscribedParamsByNode.end()) {
      subscribedNodeParamsIt->second.erase(subscribedNodeParamsIt->second.find(paramN));

      if (subscribedNodeParamsIt->second.empty()) {
        _subscribedParamsByNode.erase(subscribedNodeParamsIt);
        _paramSubscriptionsByNode.erase(_paramSubscriptionsByNode.find(nodeName));
      }
    }
  }
}

void ParameterInterface::setParamUpdateCallback(ParamUpdateFunc paramUpdateFunc) {
  std::lock_guard<std::mutex> lock(_mutex);
  _paramUpdateFunc = paramUpdateFunc;
}

ParameterList ParameterInterface::getNodeParameters(
  const rclcpp::AsyncParametersClient::SharedPtr paramClient, const std::string& nodeName,
  const std::vector<std::string>& paramNames, const std::chrono::duration<double>& timeout) {
  if (!paramClient->service_is_ready()) {
    throw std::runtime_error("Parameter service for node '" + nodeName + "' is not ready");
  }

  auto paramsToRequest = paramNames;
  if (paramsToRequest.empty()) {
    // `paramNames` is empty, list all parameter names for this node
    auto future = paramClient->list_parameters({}, 0UL);
    if (std::future_status::ready != future.wait_for(timeout)) {
      throw std::runtime_error("Failed to retrieve parameter names for node '" + nodeName + "'");
    }
    paramsToRequest = future.get().names;
  }

  // Start parameter fetches and wait for them to complete
  auto getParamsFuture = paramClient->get_parameters(paramsToRequest);
  if (std::future_status::ready != getParamsFuture.wait_for(timeout)) {
    throw std::runtime_error("Timed out waiting for " + std::to_string(paramsToRequest.size()) +
                             " parameter(s) from node '" + nodeName + "'");
  }
  const auto params = getParamsFuture.get();

  ParameterList result;
  for (const auto& param : params) {
    const auto fullParamName = prependNodeNameToParamName(param.get_name(), nodeName);
    if (isWhitelisted(fullParamName, _paramWhitelistPatterns)) {
      result.push_back(fromRosParam(rclcpp::Parameter(fullParamName, param.get_parameter_value())));
    }
  }
  return result;
}

void ParameterInterface::setNodeParameters(rclcpp::AsyncParametersClient::SharedPtr paramClient,
                                           const std::string& nodeName,
                                           const std::vector<rclcpp::Parameter>& params,
                                           const std::chrono::duration<double>& timeout) {
  if (!paramClient->service_is_ready()) {
    throw std::runtime_error("Parameter service for node '" + nodeName + "' is not ready");
  }

  auto future = paramClient->set_parameters(params);

  std::vector<std::string> paramsToDelete;
  for (const auto& p : params) {
    if (p.get_type() == rclcpp::ParameterType::PARAMETER_NOT_SET) {
      paramsToDelete.push_back(p.get_name());
    }
  }

  if (!paramsToDelete.empty()) {
    auto deleteFuture = paramClient->delete_parameters(paramsToDelete);
    if (std::future_status::ready != deleteFuture.wait_for(timeout)) {
      RCLCPP_WARN(
        _node->get_logger(),
        "Param client failed to delete %zu parameter(s) for node '%s' within the given timeout",
        paramsToDelete.size(), nodeName.c_str());
    }
  }

  if (std::future_status::ready != future.wait_for(timeout)) {
    throw std::runtime_error("Param client failed to set " + std::to_string(params.size()) +
                             " parameter(s) for node '" + nodeName + "' within the given timeout");
  }

  const auto setParamResults = future.get();
  for (auto& result : setParamResults) {
    if (!result.successful) {
      RCLCPP_WARN(_node->get_logger(), "Failed to set one or more parameters for node '%s': %s",
                  nodeName.c_str(), result.reason.c_str());
    }
  }
}

}  // namespace foxglove_bridge
