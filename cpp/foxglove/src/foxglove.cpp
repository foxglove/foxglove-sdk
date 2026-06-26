#include <foxglove-c/foxglove-c.h>
#include <foxglove/foxglove.hpp>

namespace foxglove {

void setLogLevel(LogLevel level) {
  foxglove_set_log_level(static_cast<foxglove_logging_level>(level));
}

namespace internal {

void setLibraryIdentifierPrefix(std::string_view prefix) {
  foxglove_internal_set_library_identifier_prefix({prefix.data(), prefix.size()});
}

}  // namespace internal

}  // namespace foxglove
