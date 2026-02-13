#pragma once

/// @file
/// Timestamp utilities for the data_provider example.

#include <chrono>
#include <cstdint>
#include <ctime>
#include <iomanip>
#include <optional>
#include <sstream>
#include <string>

namespace time_utils {

// ============================================================================
// Platform helpers
// ============================================================================

#ifdef _WIN32
inline std::time_t make_utc_time(std::tm* tm) {
  return _mkgmtime(tm);
}
inline std::tm to_utc_tm(std::time_t time) {
  std::tm tm{};
  gmtime_s(&tm, &time);
  return tm;
}
#else
inline std::time_t make_utc_time(std::tm* tm) {
  return timegm(tm);
}
inline std::tm to_utc_tm(std::time_t time) {
  std::tm tm{};
  gmtime_r(&time, &tm);
  return tm;
}
#endif

// ============================================================================
// ISO 8601 timestamp utilities
// ============================================================================

using TimePoint = std::chrono::system_clock::time_point;

/// Parse an ISO 8601 timestamp like "2024-01-01T00:00:00Z".
inline std::optional<TimePoint> parse_iso8601(const std::string& s) {
  std::tm tm = {};
  std::istringstream ss(s);
  ss >> std::get_time(&tm, "%Y-%m-%dT%H:%M:%S");
  if (ss.fail()) {
    return std::nullopt;
  }
  auto time = make_utc_time(&tm);
  return std::chrono::system_clock::from_time_t(time);
}

/// Format a time_point as ISO 8601.
inline std::string format_iso8601(TimePoint tp) {
  auto tt = std::chrono::system_clock::to_time_t(tp);
  std::tm tm = to_utc_tm(tt);
  std::ostringstream ss;
  ss << std::put_time(&tm, "%Y-%m-%dT%H:%M:%SZ");
  return ss.str();
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
