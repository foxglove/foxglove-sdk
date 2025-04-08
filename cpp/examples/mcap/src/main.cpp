#include <foxglove/channel.hpp>
#include <foxglove/mcap.hpp>

int main(int argc, const char* argv[]) {
  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  foxglove::McapWriter writer(options);

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
  foxglove::Channel channel{"example", "json", std::move(schema)};

  for (int i = 0; i < 100; ++i) {
    std::string msg = "{\"val\": " + std::to_string(i) + "}";
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());
  }

  // Optional, if you want to check for or handle errors
  writer.close();
}
