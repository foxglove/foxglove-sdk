/// @file
/// @brief Well-known Foxglove message type definitions.
///
/// Prefer including this header over `foxglove/schemas.hpp`, which is deprecated.

#pragma once

#include <foxglove/schemas.hpp>

namespace foxglove {
/// Preferred alias for the `foxglove::schemas` namespace, which is deprecated.
namespace messages = schemas;
}  // namespace foxglove
