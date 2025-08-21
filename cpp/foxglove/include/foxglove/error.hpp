#pragma once

#include <cstdint>
#include <iostream>
#include <sstream>

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
  /// An error related to MCAP writing.
  McapError,
  /// An error related to encoding.
  EncodeError,
  /// The provided bufffer is too short.
  BufferTooShort,
  /// Failed to decode base64 data.
  Base64DecodeError
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
using FoxgloveResult = tl::expected<T, FoxgloveError>;

/// @brief A string representation of a FoxgloveError.
///
/// @param error The error to convert to a string.
/// @return A C string representation of the error.
const char* strerror(FoxgloveError error);

/// A stream for emitting warnings to stderr, with default formatting.
/// @cond foxglove_internal
class WarnStream {
public:
  WarnStream() = default;
  WarnStream(const WarnStream&) = delete;
  WarnStream(WarnStream&&) = delete;
  WarnStream& operator=(const WarnStream&) = delete;
  WarnStream& operator=(WarnStream&&) = delete;

  template<typename T>
  WarnStream& operator<<(const T& value) {
#ifndef FOXGLOVE_DISABLE_CPP_WARNINGS
    // If T is an array type, we need special handling to avoid
    // cppcoreguidelines-pro-bounds-array-to-pointer-decay.
    if constexpr (std::is_array_v<T>) {
      // Determine the underlying element type of the array. If it's a char
      // array, treat it as a C-style string. Otherwise, it's just a pointer.
      using ElementType = std::remove_cv_t<std::remove_all_extents_t<T>>;
      if constexpr (std::is_same_v<ElementType, char>) {
        buffer_ << static_cast<const char*>(value);
      } else {
        buffer_ << static_cast<const void*>(value);
      }
    } else {
      buffer_ << value;
    }
#endif
    return *this;
  }

  ~WarnStream() {
#ifndef FOXGLOVE_DISABLE_CPP_WARNINGS
    auto msg = buffer_.str();
    std::cerr << "[foxglove] " << msg << "\n";
#endif
  }

private:
  std::ostringstream buffer_;
};

/// @private
inline WarnStream warn() {
  return WarnStream{};
}
/// @endcond

}  // namespace foxglove
