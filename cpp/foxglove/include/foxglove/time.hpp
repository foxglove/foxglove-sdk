#pragma once

#include <cstdint>

namespace foxglove::schemas {

/// @brief A timestamp in seconds and nanoseconds from the unix epoch.
struct Timestamp {
  /// @brief The number of seconds.
  uint32_t sec;
  /// @brief The number of nanoseconds.
  uint32_t nsec;
};

/// @brief A duration in seconds and nanoseconds.
struct Duration {
  /// @brief The number of seconds.
  int32_t sec;
  /// @brief The number of nanoseconds.
  uint32_t nsec;
};

}  // namespace foxglove::schemas
