#pragma once

#include <catch2/catch_test_macros.hpp>

#include <optional>

namespace foxglove_tests {

/// Asserts that an optional has a value (via Catch2 REQUIRE) and returns a reference to it.
/// Use this instead of std::optional::value() in test code to avoid
/// bugprone-unchecked-optional-access warnings while keeping clear test failure messages.
template<typename T>
T& requireValue(std::optional<T>& opt) {
  REQUIRE(opt.has_value());
  return *opt;
}

}  // namespace foxglove_tests
