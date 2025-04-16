#pragma once

#include <tl/expected.hpp>

#include <exception>
#include <memory>

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
  DuplicateChannel,
  DuplicateService,
  MissingRequestEncoding,
  ServicesNotSupported,
  ConnectionGraphNotSupported,
  IoError,
  McapError
};

template<typename T>
using FoxgloveResult = tl::expected<T, FoxgloveError>;

const char* strerror(FoxgloveError error);

}  // namespace foxglove
