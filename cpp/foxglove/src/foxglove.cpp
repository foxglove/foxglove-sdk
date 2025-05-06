#include <foxglove/foxglove.hpp>
#include <foxglove-c/foxglove-c.h>

namespace foxglove {

void setLogSeverityLevel(LogSeverityLevel level) {
    foxglove_set_log_severity_level(static_cast<foxglove_log_severity_level>(level));
}

}  // namespace foxglove
