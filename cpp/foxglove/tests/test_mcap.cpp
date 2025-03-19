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
  foxglove::McapWriter writer(options);
  writer.close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

// TODO FG-10682 add a test case for failing to open an existing file if truncate=false

TEST_CASE("specify profile") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.create = true;
  options.profile = "test_profile";
  foxglove::McapWriter writer(options);

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  foxglove::Channel channel{"example", "json", schema};
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer.close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the profile and library
  std::ifstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  std::string content((std::istreambuf_iterator<char>(file)), std::istreambuf_iterator<char>());
  printf("content: %d\n", content.size());
  REQUIRE_THAT(content, ContainsSubstring("test_profile"));
}
