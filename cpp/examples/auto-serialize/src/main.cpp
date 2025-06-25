#include <foxglove/channel.hpp>
#include <foxglove/mcap.hpp>

#include <nlohmann/json.hpp>

#include <chrono>
#include <cstddef>
#include <iostream>
#include <thread>

#include "jsonschema.hpp"

using json = nlohmann::json;

using namespace std::chrono_literals;

/**
 * Message definitions with auto-serialization.
 *
 * See https://json.nlohmann.me/features/arbitrary_types
 */
namespace messages {
enum MessageLevel {
  DEBUG,
  INFO,
};

NLOHMANN_JSON_SERIALIZE_ENUM(MessageLevel, {{DEBUG, "debug"}, {INFO, "info"}})

struct Message {
  MessageLevel level;
  std::string msg;
  int count;
};

NLOHMANN_DEFINE_TYPE_NON_INTRUSIVE(Message, level, msg, count)
}  // namespace messages

/**
 * This example writes some messages to an MCAP file, which can be opened in Foxglove and viewed in
 * the Raw Messages panel.
 *
 * Two channels are created: one with a derived JSON schema, and one using msgpack encoding (a
 * schemaless binary format).
 */
int main() {
  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.path = "auto_serialized.mcap";
  auto writer_result = foxglove::McapWriter::create(mcap_options);
  if (!writer_result.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_result.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writer_result.value());

  // 1: Channel with a JSON schema
  json msg_schema = jsonschema::generate_schema<messages::Message>();
  foxglove::Schema schema;
  schema.name = "Test";
  schema.encoding = "jsonschema";
  std::string schema_data = msg_schema.dump();
  schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  schema.data_len = schema_data.size();

  auto chan1_result = foxglove::RawChannel::create("/json", "json", schema);
  if (!chan1_result.has_value()) {
    std::cerr << "Failed to create JSON channel: " << foxglove::strerror(chan1_result.error())
              << '\n';
    return 1;
  }
  auto ch1 = std::move(chan1_result.value());

  // 2: Channel with MsgPack
  auto chan2_result = foxglove::RawChannel::create("/msgpack", "msgpack");
  if (!chan2_result.has_value()) {
    std::cerr << "Failed to create MsgPack channel: " << foxglove::strerror(chan2_result.error())
              << '\n';
    return 1;
  }
  auto ch2 = std::move(chan2_result.value());

  for (int i = 0; i < 10; i++) {
    messages::Message msg;
    msg.level = messages::MessageLevel::INFO;
    msg.msg = "Hello, World";
    msg.count = i;

    // Serialize using the macros from the messages namespace
    json value = msg;

    auto json_val = value.dump();
    ch1.log(reinterpret_cast<const std::byte*>(json_val.c_str()), json_val.size());

    // MessagePack
    std::vector<std::uint8_t> msgpack_val = json::to_msgpack(value);
    ch2.log(reinterpret_cast<const std::byte*>(msgpack_val.data()), msgpack_val.size());

    std::this_thread::sleep_for(100ms);
  }

  return 0;
}
