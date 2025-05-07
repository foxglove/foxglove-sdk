#include <foxglove/foxglove.hpp>
#include <foxglove-c/foxglove-c.h>

namespace foxglove {

void setLogLevel(LogLevel level) {
    foxglove_set_log_level(static_cast<foxglove_log_level>(level));
}

}  // namespace foxglove
