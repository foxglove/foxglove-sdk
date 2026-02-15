#pragma once

/// @file
/// Timestamp utilities for the data_provider example.

#include <date/date.h>

#include <chrono>
#include <cstdint>
#include <optional>
#include <sstream>
#include <string>

namespace time_utils {

using TimePoint = std::chrono::system_clock::time_point;

/// Parse an ISO 8601 timestamp like "2024-01-01T00:00:00Z".
inline std::optional<TimePoint> parse_iso8601(const std::string& s) {
  std::istringstream ss(s);
  TimePoint tp;
  ss >> date::parse("%FT%TZ", tp);
  if (ss.fail()) {
    return std::nullopt;
  }
  return tp;
}

/// Format a time_point as ISO 8601.
inline std::string format_iso8601(TimePoint tp) {
  return date::format("%FT%TZ", date::floor<std::chrono::seconds>(tp));
}

/// Convert time_point to nanoseconds since epoch.
inline uint64_t to_nanos(TimePoint tp) {
  auto duration = tp.time_since_epoch();
  return static_cast<uint64_t>(std::chrono::duration_cast<std::chrono::nanoseconds>(duration).count(
  ));
}

/// Round a time_point up to the next second boundary.
inline TimePoint round_up_to_second(TimePoint tp) {
  auto secs = std::chrono::duration_cast<std::chrono::seconds>(tp.time_since_epoch());
  auto rounded = TimePoint(secs);
  if (rounded < tp) {
    rounded += std::chrono::seconds(1);
  }
  return rounded;
}

}  // namespace time_utils
