#pragma once

#include <cstdint>

#include "expected.hpp"

namespace foxglove {

enum class FoxgloveError : uint8_t {
  Ok,
  Unspecified,
  ValueError,
  Utf8Error,
  SinkClosed,
  SchemaRequired,
  MessageEncodingRequired,
  ServerAlreadyStarted,
  Bind,
  DuplicateService,
  MissingRequestEncoding,
  ServicesNotSupported,
  ConnectionGraphNotSupported,
  IoError,
  McapError
};

/// @brief A result type for Foxglove operations.
///
/// This is similar to `Result` from std::expected (C++23).
///
/// You can determine if the result is successful by checking `.has_value()`. If the result is
/// successful, error will be FoxgloveError::Ok and the expected data can be unwrapped with
/// `.value()`. Otherwise, the error type can be extracted with `.error()`.
///
/// @tparam T The type of the success value returned by the operation.
template<typename T>
using FoxgloveResult = expected<T, FoxgloveError>;

/// @brief A string representation of a FoxgloveError.
///
/// @param error The error to convert to a string.
/// @return A C string representation of the error.
const char* strerror(FoxgloveError error);

}  // namespace foxglove
