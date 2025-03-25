#include <foxglove/channel.hpp>
#include <foxglove/mcap.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <filesystem>
#include <fstream>
using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

class FileCleanup {
public:
  FileCleanup(const std::string& path)
      : _path(path) {}
  ~FileCleanup() {
    if (std::filesystem::exists(_path)) {
      std::filesystem::remove(_path);
    }
  }

private:
  std::string _path;
};

TEST_CASE("Open new file and close mcap writer") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.create = true;
  foxglove::McapWriter writer(options);
  writer.close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("Open and truncate existing file") {
  FileCleanup cleanup("test.mcap");

  std::ofstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  // Write some dummy content
  const char data[] = "MCAP0000";
  file.write(data, sizeof(data) - 1);
  file.close();

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  options.create = false;
  foxglove::McapWriter writer(options);
  writer.close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

// TODO FG-10682 add a test case for failing to open an existing file if truncate=false

std::string readFile(const std::string& path) {
  std::ifstream file(path, std::ios::binary);
  REQUIRE(file.is_open());
  return std::string((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
}

TEST_CASE("Stores the schema in the channel") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  foxglove::McapWriter writer(options);

  std::string schemaJson = R"({
    "type": "object",
    "properties": {
      "my_totally_custom_schema_field": { "type": "integer" }
    },
    "required": ["my_totally_custom_schema_field"]
  })";

  const size_t dataLen = schemaJson.size();
  std::unique_ptr<std::byte[]> tempBuffer(new std::byte[dataLen]);
  std::memcpy(tempBuffer.get(), schemaJson.data(), dataLen);

  foxglove::Schema schema;
  schema.name = "TempSchema";
  schema.encoding = "jsonschema";
  schema.data = tempBuffer.get();
  schema.dataLen = dataLen;

  // Construct the channel using the temporary buffer
  foxglove::Channel channel("/temp", "json", schema);

  // Overwrite the original memory to simulate the schema lifetime ending
  std::memset(tempBuffer.get(), 0xFF, dataLen);

  const char* jsonMessage = R"({"timestamp":12345678})";
  const std::byte* messageBytes = reinterpret_cast<const std::byte*>(jsonMessage);
  size_t messageLength = std::strlen(jsonMessage);

  channel.log(messageBytes, messageLength);

  writer.close();

  REQUIRE(std::filesystem::exists("test.mcap"));
  std::string content = readFile("test.mcap");

  // Verify that the schema is present in the file
  REQUIRE(content.find(schemaJson) != std::string::npos);
}

TEST_CASE("specify profile") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.profile = "test_profile";
  foxglove::McapWriter writer(options);

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  foxglove::Channel channel{"example1", "json", schema};
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer.close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the profile and library
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("test_profile"));
}

TEST_CASE("zstd compression") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.compression = foxglove::McapCompression::Zstd;
  options.chunkSize = 10000;
  options.useChunks = true;
  foxglove::McapWriter writer(options);

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  foxglove::Channel channel{"example2", "json", schema};
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer.close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the word "zstd"
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("zstd"));
}

TEST_CASE("lz4 compression") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.compression = foxglove::McapCompression::Lz4;
  options.chunkSize = 10000;
  options.useChunks = true;
  foxglove::McapWriter writer(options);

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  foxglove::Channel channel{"example3", "json", schema};
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer.close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the word "lz4"
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("lz4"));
}
