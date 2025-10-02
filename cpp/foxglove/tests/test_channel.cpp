#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <string>

#include "common/file_cleanup.hpp"

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;
using foxglove_tests::FileCleanup;

TEST_CASE("topic is not valid utf-8") {
  auto channel =
    foxglove::RawChannel::create(std::string("\x80\x80\x80\x80"), "json", std::nullopt);
  REQUIRE(!channel.has_value());
  REQUIRE(channel.error() == foxglove::FoxgloveError::Utf8Error);
}

TEST_CASE("duplicate topic") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(channel.has_value());
  auto channel2 = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(channel2.has_value());
  REQUIRE(channel.value().id() == channel2.value().id());
  auto channel3 = foxglove::RawChannel::create("test", "msgpack", std::nullopt, context);
  REQUIRE(channel3.has_value());
  REQUIRE(channel.value().id() != channel3.value().id());
}

TEST_CASE("channel.topic()") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("/test-123", "json", std::nullopt, context);
  REQUIRE(channel.has_value());
  REQUIRE(channel.value().topic() == "/test-123");
}

TEST_CASE("channel.message_encoding()") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(channel.has_value());
  REQUIRE(channel.value().message_encoding() == "json");
}

TEST_CASE("channel.has_sinks()") {
  auto fname = "test-channel-has-sinks.mcap";
  FileCleanup cleanup(fname);

  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(channel.has_value());
  REQUIRE(!channel.value().has_sinks());

  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.context = context;
  mcap_options.path = fname;
  auto writer = foxglove::McapWriter::create(mcap_options);
  REQUIRE(writer.has_value());

  auto channel2 = foxglove::RawChannel::create("test2", "json", std::nullopt, context);
  REQUIRE(channel2.has_value());
  REQUIRE(channel2.value().has_sinks());
}

TEST_CASE("channel.close() disconnects sinks") {
  auto fname = "test-channel-close-disconnects-sinks.mcap";
  FileCleanup cleanup(fname);

  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.context = context;
  mcap_options.path = fname;
  auto writer = foxglove::McapWriter::create(mcap_options);
  REQUIRE(writer.has_value());

  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(channel.has_value());
  REQUIRE(channel.value().has_sinks());

  channel.value().close();
  REQUIRE(!channel.value().has_sinks());
}

TEST_CASE("channel.schema()") {
  foxglove::Schema mock_schema;
  mock_schema.encoding = "jsonschema";
  std::string schema_data = R"({ "type": "object", "additionalProperties": true })";
  mock_schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  mock_schema.data_len = schema_data.size();
  mock_schema.name = "test_schema";
  mock_schema.encoding = "jsonschema";

  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", mock_schema, context);
  REQUIRE(channel.has_value());

  auto schema = channel.value().schema();
  REQUIRE(schema.has_value());
  REQUIRE(schema->name == "test_schema");
  REQUIRE(schema->encoding == "jsonschema");
  REQUIRE(schema->data_len == schema_data.size());
  REQUIRE(
    std::string_view(reinterpret_cast<const char*>(schema->data), schema->data_len) == schema_data
  );
}

TEST_CASE("channel.schema() with no schema") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(channel.has_value());

  auto schema = channel.value().schema();
  REQUIRE(!schema.has_value());
}

TEST_CASE("channel with metadata") {
  auto context = foxglove::Context::create();
  std::map<std::string, std::string> metadata = {{"key1", "value1"}, {"key2", "value2"}};
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context, metadata);
  REQUIRE(channel.has_value());
  REQUIRE(channel.value().metadata().value().size() == 2);
  REQUIRE(channel.value().metadata() == metadata);
}

TEST_CASE("channel with no metadata returns an empty value from metadata()") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);

  REQUIRE(channel.has_value());
  REQUIRE(channel.value().metadata().has_value());
  REQUIRE(channel.value().metadata().value().empty());
}
