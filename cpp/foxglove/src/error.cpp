#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>

namespace foxglove {

const char* strerror(FoxgloveError error) {
  return foxglove_error_to_cstr(static_cast<foxglove_error>(error));
}

}  // namespace foxglove
