#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/schemas.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <string>

#include "common/file_cleanup.hpp"
#include "common/test_helpers.hpp"

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;
using foxglove_tests::FileCleanup;
using foxglove_tests::requireValue;

// NOLINTBEGIN(cppcoreguidelines-avoid-do-while)

TEST_CASE("topic is not valid utf-8") {
  auto channel =
    foxglove::RawChannel::create(std::string("\x80\x80\x80\x80"), "json", std::nullopt);
  REQUIRE(!channel.has_value());
  REQUIRE(channel.error() == foxglove::FoxgloveError::Utf8Error);
}

TEST_CASE("duplicate topic") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  auto channel2 = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(requireValue(channel).id() == requireValue(channel2).id());
  auto channel3 = foxglove::RawChannel::create("test", "msgpack", std::nullopt, context);
  REQUIRE(requireValue(channel).id() != requireValue(channel3).id());
}

TEST_CASE("channel.topic()") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("/test-123", "json", std::nullopt, context);
  REQUIRE(requireValue(channel).topic() == "/test-123");
}

TEST_CASE("channel.messageEncoding()") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(requireValue(channel).messageEncoding() == "json");
}

TEST_CASE("channel.hasSinks()") {
  const auto* fname = "test-channel-has-sinks.mcap";
  FileCleanup cleanup(fname);

  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(!requireValue(channel).hasSinks());

  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.context = context;
  mcap_options.path = fname;
  auto writer = foxglove::McapWriter::create(mcap_options);
  REQUIRE(writer.has_value());

  auto channel2 = foxglove::RawChannel::create("test2", "json", std::nullopt, context);
  REQUIRE(requireValue(channel2).hasSinks());
}

TEST_CASE("channel.close() disconnects sinks") {
  const auto* fname = "test-channel-close-disconnects-sinks.mcap";
  FileCleanup cleanup(fname);

  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.context = context;
  mcap_options.path = fname;
  auto writer = foxglove::McapWriter::create(mcap_options);
  REQUIRE(writer.has_value());

  auto raw_channel = foxglove::RawChannel::create("raw_test", "json", std::nullopt, context);
  REQUIRE(requireValue(raw_channel).hasSinks());

  requireValue(raw_channel).close();
  REQUIRE(!requireValue(raw_channel).hasSinks());

  auto typed_channel = foxglove::schemas::LogChannel::create("test", context);
  REQUIRE(requireValue(typed_channel).hasSinks());

  requireValue(typed_channel).close();
  REQUIRE(!requireValue(typed_channel).hasSinks());
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

  auto schema = requireValue(channel).schema();
  auto& schema_val = requireValue(schema);
  REQUIRE(schema_val.name == "test_schema");
  REQUIRE(schema_val.encoding == "jsonschema");
  REQUIRE(schema_val.data_len == schema_data.size());
  REQUIRE(
    std::string_view(reinterpret_cast<const char*>(schema_val.data), schema_val.data_len) ==
    schema_data
  );
}

TEST_CASE("channel.schema() with no schema") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);

  auto schema = requireValue(channel).schema();
  REQUIRE(!schema.has_value());
}

TEST_CASE("channel with metadata") {
  auto context = foxglove::Context::create();
  std::map<std::string, std::string> metadata = {{"key1", "value1"}, {"key2", "value2"}};
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context, metadata);
  auto chan_metadata = requireValue(channel).metadata();
  REQUIRE(requireValue(chan_metadata).size() == 2);
  REQUIRE(requireValue(channel).metadata() == metadata);
}

TEST_CASE("channel with no metadata returns an empty value from metadata()") {
  auto context = foxglove::Context::create();
  auto channel = foxglove::RawChannel::create("test", "json", std::nullopt, context);

  auto chan_metadata = requireValue(channel).metadata();
  REQUIRE(requireValue(chan_metadata).empty());
}

// NOLINTEND(cppcoreguidelines-avoid-do-while)
