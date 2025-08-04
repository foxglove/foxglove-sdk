#pragma once

#include <functional>
#include <future>
#include <optional>
#include <shared_mutex>
#include <utility>
#include <vector>

#include <nlohmann/json.hpp>
#include <websocketpp/client.hpp>
#include <websocketpp/common/memory.hpp>
#include <websocketpp/common/thread.hpp>

#include "protocol_types.hpp"

namespace foxglove::test {

inline void WriteUint32LE(uint8_t* buf, uint32_t val) {
  buf[0] = static_cast<uint8_t>(val & 0xFF);
  buf[1] = static_cast<uint8_t>((val >> 8) & 0xFF);
  buf[2] = static_cast<uint8_t>((val >> 16) & 0xFF);
  buf[3] = static_cast<uint8_t>((val >> 24) & 0xFF);
}

inline uint32_t ReadUint32LE(const uint8_t* buf) {
  return static_cast<uint32_t>(buf[0]) | (static_cast<uint32_t>(buf[1]) << 8) |
         (static_cast<uint32_t>(buf[2]) << 16) | (static_cast<uint32_t>(buf[3]) << 24);
}

inline void from_json(const nlohmann::json& j, foxglove::ParameterValue& p) {
  if (j.is_string()) {
    p = foxglove::ParameterValue(j.get<std::string>());
  } else if (j.is_number_integer()) {
    p = foxglove::ParameterValue(j.get<int64_t>());
  } else if (j.is_number_float()) {
    p = foxglove::ParameterValue(j.get<double>());
  } else if (j.is_boolean()) {
    p = foxglove::ParameterValue(j.get<bool>());
  } else if (j.is_array()) {
    std::vector<foxglove::ParameterValue> values;
    for (const auto& value : j) {
      foxglove::ParameterValue paramValue("dummy");
      from_json(value, paramValue);
      values.emplace_back(std::move(paramValue));
    }
    p = foxglove::ParameterValue(std::move(values));
  } else {
    throw std::runtime_error("Encountered unknown type for ParameterValue");
  }
}

inline void from_json(const nlohmann::json& j, foxglove::Parameter& p) {
  std::string name = j["name"].get<std::string>();
  if (j["value"].is_string()) {
    p = foxglove::Parameter(name, j["value"].get<std::string>());
  } else if (j["value"].is_number_integer()) {
    p = foxglove::Parameter(name, j["value"].get<int64_t>());
  } else if (j["value"].is_number_float()) {
    p = foxglove::Parameter(name, j["value"].get<double>());
  } else if (j["value"].is_boolean()) {
    p = foxglove::Parameter(name, j["value"].get<bool>());
  } else if (j["value"].is_array()) {
    std::vector<foxglove::ParameterValue> values;
    for (const auto& value : j["value"]) {
      foxglove::ParameterValue paramValue("dummy");
      from_json(value, paramValue);
      values.emplace_back(std::move(paramValue));
    }
    foxglove::ParameterType type = foxglove::ParameterType::None;
    if (j.contains("type")) {
      std::string typeStr = j["type"].get<std::string>();
      if (typeStr == "float64") {
        type = foxglove::ParameterType::Float64;
      } else if (typeStr == "float64_array") {
        type = foxglove::ParameterType::Float64Array;
      } else if (typeStr == "byte_array") {
        type = foxglove::ParameterType::ByteArray;
      } else {
        throw std::runtime_error("Encountered unknown type: " + typeStr);
      }
    }

    p = foxglove::Parameter(name, type, foxglove::ParameterValue(std::move(values)));
  } else {
    throw std::runtime_error("Encountered unknown type for parameter " +
                             j["name"].get<std::string>());
  }
}

inline void from_json(const nlohmann::json& j, std::vector<foxglove::Parameter>& parameters) {
  for (const auto& parameter : j) {
    // Required to pass by reference to from_json, actual initialization will happen there
    foxglove::Parameter param("dummy");
    from_json(parameter, param);
    parameters.push_back(std::move(param));
  }
}

inline void to_json(nlohmann::json& j, const foxglove::ParameterValueView& p) {
  if (p.is<std::string>()) {
    j = p.get<std::string>();
  } else if (p.is<int64_t>()) {
    j = p.get<int64_t>();
  } else if (p.is<double>()) {
    j = p.get<double>();
  } else if (p.is<bool>()) {
    j = p.get<bool>();
  } else if (p.is<std::vector<foxglove::ParameterValueView>>()) {
    j = nlohmann::json::array();
    for (const auto& value : p.get<std::vector<foxglove::ParameterValueView>>()) {
      nlohmann::json valueJson;
      to_json(valueJson, value);
      j.push_back(std::move(valueJson));
    }
  } else {
    throw std::runtime_error("Encountered unknown type for ParameterValueView");
  }
}

inline void to_json(nlohmann::json& j, const foxglove::ParameterValue& p) {
  to_json(j, p.view());
}

inline void to_json(nlohmann::json& j, const foxglove::Parameter& p) {
  j["name"] = p.name();
  if (p.hasValue()) {
    to_json(j["value"], p.value().value());
  }
}

inline void to_json(nlohmann::json& j, const std::vector<foxglove::Parameter>& parameters) {
  j = nlohmann::json::array();
  for (const auto& parameter : parameters) {
    nlohmann::json parameterJson;
    to_json(parameterJson, parameter);
    j.push_back(std::move(parameterJson));
  }
}

inline void to_json(nlohmann::json& j, const ClientAdvertisement& p) {
  j = nlohmann::json{{"id", p.channelId},
                     {"topic", p.topic},
                     {"encoding", p.encoding},
                     {"schemaName", p.schemaName}};
}

inline void from_json(const nlohmann::json& j, Channel& c) {
  c.id = j["id"].get<uint32_t>();
  c.topic = j["topic"].get<std::string>();
  c.encoding = j["encoding"].get<std::string>();
  c.schemaName = j["schemaName"].get<std::string>();
  c.schema = j["schema"].get<std::string>();
  if (j.find("schemaEncoding") != j.end()) {
    c.schemaEncoding = j["schemaEncoding"].get<std::string>();
  }
}

inline void from_json(const nlohmann::json& j, Service& s) {
  s.id = j["id"].get<uint32_t>();
  s.name = j["name"].get<std::string>();
  s.type = j["type"].get<std::string>();
  s.requestType = j["request"]["schemaName"].get<std::string>();
  s.requestSchema = j["request"]["schema"].get<std::string>();
  s.responseType = j["response"]["schemaName"].get<std::string>();
  s.responseSchema = j["response"]["schema"].get<std::string>();
}

using TextMessageHandler = std::function<void(const std::string&)>;
using BinaryMessageHandler = std::function<void(const uint8_t*, size_t)>;
using OpCode = websocketpp::frame::opcode::value;

template <typename ClientConfiguration>
class Client {
public:
  using ClientType = websocketpp::client<ClientConfiguration>;
  using MessagePtr = typename ClientType::message_ptr;
  using ConnectionPtr = typename ClientType::connection_ptr;

  Client() {
    _endpoint.clear_access_channels(websocketpp::log::alevel::all);
    _endpoint.clear_error_channels(websocketpp::log::elevel::all);

    _endpoint.init_asio();
    _endpoint.start_perpetual();

    _endpoint.set_message_handler(
      bind(&Client::messageHandler, this, std::placeholders::_1, std::placeholders::_2));

    _thread.reset(new websocketpp::lib::thread(&ClientType::run, &_endpoint));
  }

  virtual ~Client() {
    close();
    _endpoint.stop_perpetual();
    _thread->join();
  }

  void connect(const std::string& uri,
               std::function<void(websocketpp::connection_hdl)> onOpenHandler,
               std::function<void(websocketpp::connection_hdl)> onCloseHandler = nullptr) {
    std::unique_lock<std::shared_mutex> lock(_mutex);

    websocketpp::lib::error_code ec;
    _con = _endpoint.get_connection(uri, ec);

    if (ec) {
      throw std::runtime_error("Failed to get connection from URI " + uri);
    }

    if (onOpenHandler) {
      _con->set_open_handler(onOpenHandler);
    }
    if (onCloseHandler) {
      _con->set_close_handler(onCloseHandler);
    }

    _con->add_subprotocol(SUPPORTED_SUBPROTOCOL);
    _endpoint.connect(_con);
  }

  std::future<void> connect(const std::string& uri) {
    auto promise = std::make_shared<std::promise<void>>();
    auto future = promise->get_future();

    connect(uri, [p = std::move(promise)](websocketpp::connection_hdl) mutable {
      p->set_value();
    });

    return future;
  }

  void close() {
    std::unique_lock<std::shared_mutex> lock(_mutex);
    if (!_con) {
      return;  // Already disconnected
    }

    _endpoint.close(_con, websocketpp::close::status::going_away, "");
    _con.reset();
  }

  void messageHandler(websocketpp::connection_hdl hdl, MessagePtr msg) {
    (void)hdl;
    const OpCode op = msg->get_opcode();

    switch (op) {
      case OpCode::TEXT: {
        std::shared_lock<std::shared_mutex> lock(_mutex);
        if (_textMessageHandler) {
          _textMessageHandler(msg->get_payload());
        }
      } break;
      case OpCode::BINARY: {
        std::shared_lock<std::shared_mutex> lock(_mutex);
        const auto& payload = msg->get_payload();
        if (_binaryMessageHandler) {
          _binaryMessageHandler(reinterpret_cast<const uint8_t*>(payload.data()), payload.size());
        }
      } break;
      default:
        break;
    }
  }

  void subscribe(const std::vector<std::pair<SubscriptionId, ChannelId>>& subscriptions) {
    nlohmann::json subscriptionsJson;
    for (const auto& [subId, channelId] : subscriptions) {
      subscriptionsJson.push_back({{"id", subId}, {"channelId", channelId}});
    }

    const std::string payload =
      nlohmann::json{{"op", "subscribe"}, {"subscriptions", std::move(subscriptionsJson)}}.dump();
    sendText(payload);
  }

  void unsubscribe(const std::vector<SubscriptionId>& subscriptionIds) {
    const std::string payload =
      nlohmann::json{{"op", "unsubscribe"}, {"subscriptionIds", subscriptionIds}}.dump();
    sendText(payload);
  }

  void advertise(const std::vector<ClientAdvertisement>& channels) {
    const std::string payload = nlohmann::json{{"op", "advertise"}, {"channels", channels}}.dump();
    sendText(payload);
  }

  void unadvertise(const std::vector<ClientChannelId>& channelIds) {
    const std::string payload =
      nlohmann::json{{"op", "unadvertise"}, {"channelIds", channelIds}}.dump();
    sendText(payload);
  }

  void publish(ClientChannelId channelId, const uint8_t* buffer, size_t size) {
    std::vector<uint8_t> payload(1 + 4 + size);
    payload[0] = uint8_t(ClientBinaryOpcode::MESSAGE_DATA);
    WriteUint32LE(payload.data() + 1, channelId);
    std::memcpy(payload.data() + 1 + 4, buffer, size);
    sendBinary(payload.data(), payload.size());
  }

  void sendServiceRequest(const ServiceRequest& request) {
    size_t payloadSize = 1 + 4 + 4 + 4 + request.encoding.size() + request.data.size();
    std::vector<uint8_t> payload(payloadSize);

    payload[0] = uint8_t(ClientBinaryOpcode::SERVICE_CALL_REQUEST);
    size_t offset = 1;
    WriteUint32LE(payload.data() + offset, request.serviceId);
    offset += 4;
    WriteUint32LE(payload.data() + offset, request.callId);
    offset += 4;
    uint32_t encodingLength = request.encoding.size();
    WriteUint32LE(payload.data() + offset, encodingLength);
    offset += 4;
    std::memcpy(payload.data() + offset, request.encoding.data(), encodingLength);
    offset += encodingLength;

    if (request.data.size() > 0) {
      std::memcpy(payload.data() + offset, request.data.data(), request.data.size());
    }

    sendBinary(payload.data(), payload.size());
  }

  void getParameters(const std::vector<std::string>& parameterNames,
                     const std::optional<std::string>& requestId = std::nullopt) {
    nlohmann::json jsonPayload{{"op", "getParameters"}, {"parameterNames", parameterNames}};
    if (requestId) {
      jsonPayload["id"] = requestId.value();
    }
    sendText(jsonPayload.dump());
  }

  void setParameters(const std::vector<foxglove::Parameter>& parameters,
                     const std::optional<std::string>& requestId = std::nullopt) {
    nlohmann::json parametersJson;
    to_json(parametersJson, parameters);
    nlohmann::json jsonPayload{{"op", "setParameters"}, {"parameters", parametersJson}};
    if (requestId) {
      jsonPayload["id"] = requestId.value();
    }
    sendText(jsonPayload.dump());
  }

  void subscribeParameterUpdates(const std::vector<std::string>& parameterNames) {
    nlohmann::json jsonPayload{{"op", "subscribeParameterUpdates"},
                               {"parameterNames", parameterNames}};
    sendText(jsonPayload.dump());
  }

  void unsubscribeParameterUpdates(const std::vector<std::string>& parameterNames) {
    nlohmann::json jsonPayload{{"op", "unsubscribeParameterUpdates"},
                               {"parameterNames", parameterNames}};
    sendText(jsonPayload.dump());
  }

  void fetchAsset(const std::string& uri, uint32_t requestId) {
    nlohmann::json jsonPayload{{"op", "fetchAsset"}, {"uri", uri}, {"requestId", requestId}};
    sendText(jsonPayload.dump());
  }

  void setTextMessageHandler(TextMessageHandler handler) {
    std::unique_lock<std::shared_mutex> lock(_mutex);
    _textMessageHandler = std::move(handler);
  }

  void setBinaryMessageHandler(BinaryMessageHandler handler) {
    std::unique_lock<std::shared_mutex> lock(_mutex);
    _binaryMessageHandler = std::move(handler);
  }

  void sendText(const std::string& payload) {
    std::shared_lock<std::shared_mutex> lock(_mutex);
    _endpoint.send(_con, payload, OpCode::TEXT);
  }

  void sendBinary(const uint8_t* data, size_t dataLength) {
    std::shared_lock<std::shared_mutex> lock(_mutex);
    _endpoint.send(_con, data, dataLength, OpCode::BINARY);
  }

  std::future<std::vector<uint8_t>> waitForChannelMsg(SubscriptionId subscriptionId) {
    // Set up binary message handler to resolve when a binary message has been received
    auto promise = std::make_shared<std::promise<std::vector<uint8_t>>>();
    auto future = promise->get_future();

    setBinaryMessageHandler(
      [promise = std::move(promise), subscriptionId](const uint8_t* data, size_t dataLength) {
        if (ReadUint32LE(data + 1) != subscriptionId) {
          return;
        }
        const size_t offset = 1 + 4 + 8;
        std::vector<uint8_t> dataCopy(dataLength - offset);
        std::memcpy(dataCopy.data(), data + offset, dataLength - offset);
        promise->set_value(std::move(dataCopy));
      });

    return future;
  }

  std::future<std::vector<foxglove::Parameter>> waitForParameters(
    const std::string& requestId = std::string()) {
    auto promise = std::make_shared<std::promise<std::vector<foxglove::Parameter>>>();
    auto future = promise->get_future();

    setTextMessageHandler([promise = std::move(promise), requestId](const std::string& payload) {
      const auto msg = nlohmann::json::parse(payload);
      const auto& op = msg["op"].get<std::string>();
      const auto id = msg.value("id", "");

      if (op == "parameterValues" && (requestId.empty() || requestId == id)) {
        std::vector<foxglove::Parameter> parameters;
        from_json(msg["parameters"], parameters);
        promise->set_value(std::move(parameters));
      }
    });

    return future;
  }

  std::future<ServiceResponse> waitForServiceResponse() {
    auto promise = std::make_shared<std::promise<ServiceResponse>>();
    auto future = promise->get_future();

    setBinaryMessageHandler([promise = std::move(promise)](const uint8_t* data,
                                                           size_t dataLength) mutable {
      if (static_cast<ServerBinaryOpcode>(data[0]) != ServerBinaryOpcode::SERVICE_CALL_RESPONSE) {
        return;
      }

      // Deserialize response
      ServiceResponse response;
      size_t offset = 1;
      response.serviceId = ReadUint32LE(data + offset);
      offset += 4;
      response.callId = ReadUint32LE(data + offset);
      offset += 4;
      const size_t encodingLength = ReadUint32LE(data + offset);
      offset += 4;
      response.encoding = std::string(reinterpret_cast<const char*>(data + offset), encodingLength);
      offset += encodingLength;
      const auto payloadLength = dataLength - offset;
      response.data.resize(payloadLength);
      std::memcpy(response.data.data(), data + offset, payloadLength);

      promise->set_value(response);
    });
    return future;
  }

  std::future<Service> waitForService(const std::string& serviceName) {
    auto promise = std::make_shared<std::promise<Service>>();
    auto future = promise->get_future();

    setTextMessageHandler(
      [promise = std::move(promise), serviceName](const std::string& payload) mutable {
        const auto msg = nlohmann::json::parse(payload);
        const auto& op = msg["op"].get<std::string>();

        if (op == "advertiseServices") {
          const auto services = msg["services"].get<std::vector<Service>>();
          for (const auto& service : services) {
            if (service.name == serviceName) {
              promise->set_value(service);
              break;
            }
          }
        }
      });

    return future;
  }

  std::future<Channel> waitForChannel(const std::string& topicName) {
    auto promise = std::make_shared<std::promise<Channel>>();
    auto future = promise->get_future();

    setTextMessageHandler(
      [promise = std::move(promise), topicName](const std::string& payload) mutable {
        const auto msg = nlohmann::json::parse(payload);
        const auto& op = msg["op"].get<std::string>();

        if (op == "advertise") {
          const auto channels = msg["channels"].get<std::vector<Channel>>();
          for (const auto& channel : channels) {
            if (channel.topic == topicName) {
              promise->set_value(channel);
              break;
            }
          }
        }
      });
    return future;
  }

  std::future<FetchAssetResponse> waitForFetchAssetResponse() {
    auto promise = std::make_shared<std::promise<FetchAssetResponse>>();
    auto future = promise->get_future();

    setBinaryMessageHandler(
      [promise = std::move(promise)](const uint8_t* data, size_t dataLength) mutable {
        if (static_cast<ServerBinaryOpcode>(data[0]) != ServerBinaryOpcode::FETCH_ASSET_RESPONSE) {
          return;
        }

        FetchAssetResponse response;
        size_t offset = 1;
        response.requestId = ReadUint32LE(data + offset);
        offset += 4;
        response.status = static_cast<FetchAssetStatus>(data[offset]);
        offset += 1;
        const size_t errorMsgLength = static_cast<size_t>(ReadUint32LE(data + offset));
        offset += 4;
        response.errorMessage =
          std::string(reinterpret_cast<const char*>(data + offset), errorMsgLength);
        offset += errorMsgLength;
        const auto payloadLength = dataLength - offset;

        response.data.resize(payloadLength);
        if (payloadLength > 0) {
          std::memcpy(response.data.data(), data + offset, payloadLength);
        }

        promise->set_value(response);
      });
    return future;
  }

protected:
  static constexpr char SUPPORTED_SUBPROTOCOL[] = "foxglove.sdk.v1";
  ClientType _endpoint;
  websocketpp::lib::shared_ptr<websocketpp::lib::thread> _thread;
  ConnectionPtr _con;
  std::shared_mutex _mutex;
  TextMessageHandler _textMessageHandler;
  BinaryMessageHandler _binaryMessageHandler;
};

}  // namespace foxglove::test
