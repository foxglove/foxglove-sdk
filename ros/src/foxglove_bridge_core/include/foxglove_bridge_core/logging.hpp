#pragma once

#include <cstdarg>
#include <cstdio>
#include <functional>
#include <string>
#include <utility>

namespace foxglove_bridge {

enum class BridgeLogLevel {
  Debug,
  Info,
  Warn,
  Error,
  Fatal,
};

/// Sink for core log output. Frontends route this to their ROS logger
/// (RCLCPP_* / ROS_*).
using LogFn = std::function<void(BridgeLogLevel, const std::string& message)>;

/// printf-style convenience wrapper around a LogFn.
class Logger {
public:
  Logger() = default;
  explicit Logger(LogFn fn)
      : _fn(std::move(fn)) {}

#if defined(__GNUC__) || defined(__clang__)
  __attribute__((format(printf, 3, 4)))
#endif
  void
  log(BridgeLogLevel level, const char* fmt, ...) const {
    if (!_fn) {
      return;
    }
    va_list args;
    va_start(args, fmt);
    va_list argsCopy;
    va_copy(argsCopy, args);
    const int size = vsnprintf(nullptr, 0, fmt, args);
    va_end(args);
    std::string message;
    if (size > 0) {
      message.resize(static_cast<size_t>(size));
      // size + 1: vsnprintf writes the terminator into the extra byte that
      // std::string guarantees past the end of its data.
      vsnprintf(message.data(), static_cast<size_t>(size) + 1, fmt, argsCopy);
    }
    va_end(argsCopy);
    _fn(level, message);
  }

  explicit operator bool() const {
    return static_cast<bool>(_fn);
  }

private:
  LogFn _fn;
};

}  // namespace foxglove_bridge
