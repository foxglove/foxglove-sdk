#pragma once

#include <cstdint>

namespace foxglove {

/**
 * The severity level for stderr logging from the SDK.
 */
enum class LogSeverityLevel : uint8_t {
    Off = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
};

/**
 * Set the severity level for SDK stderr logging.
 *
 * The SDK logs informational messages to stderr. Any messages below the filtered level are not
 * logged. This does not affect logging of messages to MCAP or Foxglove.
 *
 * This function should be called before other Foxglove initialization to capture output from all
 * components.
 *
 * By default, stderr logging is disabled.
 */
void setLogSeverityLevel(LogSeverityLevel level);

}  // namespace foxglove
