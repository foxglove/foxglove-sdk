#pragma once

#include <cstdint>
#include <iostream>
#include <sstream>

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
  McapError,
  BufferTooShort,
  Base64DecodeError
};

template<typename T>
using FoxgloveResult = expected<T, FoxgloveError>;

const char* strerror(FoxgloveError error);

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

inline WarnStream warn() {
  return WarnStream{};
}

}  // namespace foxglove
