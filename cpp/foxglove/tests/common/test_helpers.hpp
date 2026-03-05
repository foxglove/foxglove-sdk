#pragma once

#include <catch2/catch_test_macros.hpp>

#include <optional>

#include <foxglove/expected.hpp>

namespace foxglove_tests {

/// Asserts that an optional has a value (via Catch2 REQUIRE) and returns a reference to it.
/// Use this instead of std::optional::value() in test code to avoid
/// bugprone-unchecked-optional-access warnings while keeping clear test failure messages.
template<typename T>
T& requireValue(std::optional<T>& opt) {
  REQUIRE(opt.has_value());
  return *opt;
}

template<typename T>
const T& requireValue(const std::optional<T>& opt) {
  REQUIRE(opt.has_value());
  return *opt;
}

/// Asserts that a tl::expected has a value (via Catch2 REQUIRE) and returns a reference to it.
/// Use this instead of tl::expected::value() in test code to avoid
/// bugprone-unchecked-optional-access warnings while keeping clear test failure messages.
template<typename T, typename E>
T& requireValue(tl::expected<T, E>& exp) {
  REQUIRE(exp.has_value());
  return *exp;
}

template<typename T, typename E>
const T& requireValue(const tl::expected<T, E>& exp) {
  REQUIRE(exp.has_value());
  return *exp;
}

}  // namespace foxglove_tests
