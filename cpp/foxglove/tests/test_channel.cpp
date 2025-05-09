#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <string>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

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
