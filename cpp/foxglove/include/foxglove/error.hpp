#pragma once

#include <cstdint>

#include "expected.hpp"

/// The foxglove namespace.
namespace foxglove {

///
/// Error codes which may be returned in a FoxgloveResult.
///
enum class FoxgloveError : uint8_t {
  /// The operation was successful.
  Ok,
  /// An unspecified error.
  Unspecified,
  /// A value or argument is invalid.
  ValueError,
  /// A UTF-8 error.
  Utf8Error,
  /// The sink dropped a message because it is closed.
  SinkClosed,
  /// A schema is required.
  SchemaRequired,
  /// A message encoding is required.
  MessageEncodingRequired,
  /// The server is already started.
  ServerAlreadyStarted,
  /// Failed to bind to the specified host and port.
  Bind,
  /// A service with the same name is already registered.
  DuplicateService,
  /// Neither the service nor the server declared supported encodings.
  MissingRequestEncoding,
  /// Services are not supported on this server instance.
  ServicesNotSupported,
  /// Connection graph is not supported on this server instance.
  ConnectionGraphNotSupported,
  /// An I/O error.
  IoError,
  /// An error related to MCAP encoding.
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
