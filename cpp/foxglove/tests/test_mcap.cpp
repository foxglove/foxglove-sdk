#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <array>
#include <filesystem>
#include <fstream>
#include <optional>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

class FileCleanup {
public:
  explicit FileCleanup(std::string&& path)
      : path_(std::move(path)) {}
  FileCleanup(const FileCleanup&) = delete;
  FileCleanup& operator=(const FileCleanup&) = delete;
  FileCleanup(FileCleanup&&) = delete;
  FileCleanup& operator=(FileCleanup&&) = delete;
  ~FileCleanup() {
    if (std::filesystem::exists(path_)) {
      std::filesystem::remove(path_);
    }
  }

private:
  std::string path_;
};

TEST_CASE("Open new file and close mcap writer") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());
  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("Open and truncate existing file") {
  FileCleanup cleanup("test.mcap");

  std::ofstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  // Write some dummy content
  const char* data = "MCAP0000";
  file.write(data, 8);
  file.close();

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());
  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("fail to open existing file if truncate=false") {
  FileCleanup cleanup("test.mcap");

  std::ofstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  // Write some dummy content
  const char* data = "MCAP0000";
  file.write(data, 8);
  file.close();

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::IoError);

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("fail to open existing file if create=true and truncate=false") {
  FileCleanup cleanup("test.mcap");

  std::ofstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  // Write some dummy content
  const char* data = "MCAP0000";
  file.write(data, 8);
  file.close();

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::IoError);

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("fail if file path is not valid utf-8") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "\x80\x80\x80\x80";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::Utf8Error);

  // Check test.mcap file does not exist
  REQUIRE(!std::filesystem::exists("test.mcap"));
}

std::string readFile(const std::string& path) {
  std::ifstream file(path, std::ios::binary);
  REQUIRE(file.is_open());
  return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

TEST_CASE("different contexts") {
  FileCleanup cleanup("test.mcap");
  auto context1 = foxglove::Context::create();
  auto context2 = foxglove::Context::create();

  // Create writer on context1
  foxglove::McapWriterOptions options{context1};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Log on context2 (should not be output to the file)
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::Channel::create("example1", "json", schema, context2);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it does not contain the message
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, !ContainsSubstring("Hello, world!"));
}

TEST_CASE("specify profile") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options{context};
  options.path = "test.mcap";
  options.profile = "test_profile";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::Channel::create("example1", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto& channel = channel_result.value();
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the profile and library
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("test_profile"));
}

TEST_CASE("zstd compression") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options{context};
  options.path = "test.mcap";
  options.compression = foxglove::McapCompression::Zstd;
  options.chunkSize = 10000;
  options.useChunks = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::Channel::create("example2", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the word "zstd"
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("zstd"));
}

TEST_CASE("lz4 compression") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options{context};
  options.path = "test.mcap";
  options.compression = foxglove::McapCompression::Lz4;
  options.chunkSize = 10000;
  options.useChunks = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::Channel::create("example3", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto& channel = channel_result.value();
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  auto error = writer->close();
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the word "lz4"
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("lz4"));
}

TEST_CASE("Channel can outlive Schema") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options{context};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  std::optional<foxglove::Channel> channel;
  {
    foxglove::Schema schema;
    schema.name = "ExampleSchema";
    schema.encoding = "unknown";
    std::string data = "FAKESCHEMA";
    schema.data = reinterpret_cast<const std::byte*>(data.data());
    schema.data_len = data.size();
    auto result = foxglove::Channel::create("example", "json", schema, context);
    REQUIRE(result.has_value());
    // Channel should copy the schema, so this modification has no effect on the output
    data[2] = 'I';
    data[3] = 'L';
    // Use emplace to construct the optional directly
    channel.emplace(std::move(result.value()));
  }

  const std::array<uint8_t, 3> data = {4, 5, 6};
  channel->log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  REQUIRE(std::filesystem::exists("test.mcap"));

  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, !ContainsSubstring("FAILSCHEMA"));
  REQUIRE_THAT(content, ContainsSubstring("FAKESCHEMA"));
}
