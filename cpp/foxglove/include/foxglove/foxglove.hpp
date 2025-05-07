#pragma once

#include <cstdint>

namespace foxglove {

/**
 * The severity level for stderr logging from the SDK.
 */
enum class LogLevel : uint8_t {
  Off = 0,
  Debug = 1,
  Info = 2,
  Warn = 3,
  Error = 4,
};

/// @brief Initialize SDK logging with the given severity level.
///
/// The SDK logs informational messages to stderr. Any messages below the given level are not
/// logged. Note that this does not affect logging of messages to MCAP or Foxglove.
///
/// This function should be called before other Foxglove initialization to capture output from all
/// components. Subsequent calls will have no effect.
///
/// As long as you initialize one logging sink (WebSocket server or MCAP), log level may instead be
/// configured via a `FOXGLOVE_LOG_LEVEL` environment variable, with one of the values "debug",
/// "info", "warn", or "error". Default is "info".
///
/// Additionally, you may control whether style characters such as colors are included in log output
/// via the `FOXGLOVE_LOG_STYLE` environment variable. Valid values are "never", "always", and
/// "auto". "auto" will attempt to print styles where supported; this is the default.
///
/// If this method is not called, and neither of the environment variables are set, this logging is
/// disabled.
///
/// @param level The severity level for stderr logging from the SDK.
void setLogLevel(LogLevel level);

}  // namespace foxglove
