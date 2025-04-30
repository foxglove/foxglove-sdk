#include <foxglove/channel.hpp>
#include <foxglove/mcap.hpp>

#include <iostream>

int main(int argc, const char* argv[]) {
  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  auto writerResult = foxglove::McapWriter::create(options);
  if (!writerResult.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writerResult.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writerResult.value());

  foxglove::Schema schema;
  schema.name = "Test";
  schema.encoding = "jsonschema";
  std::string schemaData = R"({
    "type": "object",
    "properties": {
        "val": { "type": "number" }
    }
    })";
  schema.data = reinterpret_cast<const std::byte*>(schemaData.data());
  schema.dataLen = schemaData.size();
  auto channelResult = foxglove::RawChannel::create("example", "json", std::move(schema));
  if (!channelResult.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channelResult.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channelResult.value());

  for (int i = 0; i < 100; ++i) {
    std::string msg = "{\"val\": " + std::to_string(i) + "}";
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());
  }

  // Optional, if you want to check for or handle errors
  writer.close();
  return 0;
}
