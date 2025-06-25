#include <foxglove/channel.hpp>
#include <foxglove/mcap.hpp>

#include <iostream>

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  auto writer_result = foxglove::McapWriter::create(options);
  if (!writer_result.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_result.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writer_result.value());

  foxglove::Schema schema;
  schema.name = "Test";
  schema.encoding = "jsonschema";
  std::string schema_data = R"({
    "type": "object",
    "properties": {
        "val": { "type": "number" }
    }
    })";
  schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  schema.data_len = schema_data.size();
  auto channel_result = foxglove::RawChannel::create("example", "json", std::move(schema));
  if (!channel_result.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channel_result.value());

  for (int i = 0; i < 100; ++i) {
    std::string msg = "{\"val\": " + std::to_string(i) + "}";
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());
  }

  // Optional, if you want to check for or handle errors
  foxglove::FoxgloveError err = writer.close();
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to close writer: " << foxglove::strerror(err) << '\n';
    return 1;
  }
  return 0;
}
