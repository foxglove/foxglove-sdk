#pragma once

/// @cond foxglove_internal

#include <foxglove-c/foxglove-c.h>
#include <foxglove/mcap.hpp>

namespace foxglove {

/// Convert C++ McapWriterOptions to C foxglove_mcap_options.
foxglove_mcap_options to_c_mcap_options(const McapWriterOptions& options);

}  // namespace foxglove

/// @endcond
