#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <string>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

TEST_CASE("topic is not valid utf-8") {
  auto channel = foxglove::Channel::create(std::string("\x80\x80\x80\x80"), "json", std::nullopt);
  REQUIRE(!channel.has_value());
  REQUIRE(channel.error() == foxglove::FoxgloveError::Utf8Error);
}

// TODO FG-11089: create a context specifically for this test here so it doesn't pollute the global
// context

TEST_CASE("duplicate topic") {
  auto channel = foxglove::Channel::create("test", "json", std::nullopt);
  REQUIRE(channel.has_value());
  auto channel2 = foxglove::Channel::create("test", "json", std::nullopt);
  REQUIRE(channel2.has_value());
  REQUIRE(channel.value().id() != channel2.value().id());
}
