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

  // If you want to add some MCAP metadata: https://mcap.dev/spec#metadata-op0x0c
  std::map<std::string, std::string> metadata = {{"os", "linux"}, {"arch", "x64"}};
  foxglove::FoxgloveError err = writer.writeMetadata("platform", metadata.begin(), metadata.end());
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to write metadata: " << foxglove::strerror(err) << std::endl;
    return 1;
  }

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
  err = writer.close();
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to close writer: " << foxglove::strerror(err) << '\n';
    return 1;
  }
  return 0;
}
