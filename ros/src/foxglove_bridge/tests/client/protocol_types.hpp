#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <variant>
#include <vector>

#include <foxglove/server/parameter.hpp>

namespace foxglove::test {

// Protocol type aliases
using ChannelId = uint64_t;
using SubscriptionId = uint32_t;
using ClientChannelId = uint32_t;
using ServiceId = uint32_t;
using CallId = uint32_t;

// Protocol opcode and status enums
enum class ClientBinaryOpcode : uint8_t {
  MESSAGE_DATA = 1,
  SERVICE_CALL_REQUEST = 2,
};

enum class ServerBinaryOpcode : uint8_t {
  MESSAGE_DATA = 1,
  TIME = 2,
  SERVICE_CALL_RESPONSE = 3,
  FETCH_ASSET_RESPONSE = 4,
};

enum class FetchAssetStatus : uint8_t {
  Success = 0,
  Error = 1,
};

// Stub data structures corresponding to protocol messages. Used to wrap the values returned by the
// server and use them in test assertions.
struct Channel {
  ChannelId id;
  std::string topic;
  std::string encoding;
  std::string schemaName;
  std::string schema;
  std::optional<std::string> schemaEncoding;
};

struct Service {
  ServiceId id;
  std::string name;
  std::string type;
  std::string requestType;
  std::string requestSchema;
  std::string responseType;
  std::string responseSchema;
};

struct ServiceRequest {
  ServiceId serviceId;
  CallId callId;
  std::string encoding;
  std::vector<std::byte> data;
};

struct ServiceResponse {
  ServiceId serviceId;
  CallId callId;
  std::string encoding;
  std::vector<std::byte> data;
};

struct FetchAssetResponse {
  uint32_t requestId;
  FetchAssetStatus status;
  std::string errorMessage;
  std::vector<uint8_t> data;
};

struct ClientAdvertisement {
  ClientChannelId channelId;
  std::string topic;
  std::string encoding;
  std::string schemaName;
  std::vector<char> schema;
};

}  // namespace foxglove::test
