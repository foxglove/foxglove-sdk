#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <string>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

TEST_CASE("topic is not valid utf-8") {
  try {
    foxglove::Channel channel(std::string("\x80\x80\x80\x80"), "json", std::nullopt);
    REQUIRE(false);  // expected error
  } catch (const foxglove::FoxgloveError& e) {
    REQUIRE(e.kind() == foxglove::FoxgloveErrorKind::ValueError);
    REQUIRE_THAT(e.what(), ContainsSubstring("invalid utf-8"));
  }
}

// TODO FG-11089: create a context specifically for this test here so it doesn't pollute the global
// context

TEST_CASE("duplicate topic") {
  foxglove::Channel channel("test", "json", std::nullopt);
  try {
    foxglove::Channel channel("test", "json", std::nullopt);
    REQUIRE(false);  // expected error
  } catch (const foxglove::FoxgloveError& e) {
    REQUIRE(e.kind() == foxglove::FoxgloveErrorKind::DuplicateChannel);
    REQUIRE_THAT(e.what(), ContainsSubstring("topic test already exists"));
  }
}
