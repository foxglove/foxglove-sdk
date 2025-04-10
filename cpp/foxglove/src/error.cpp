#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>

namespace foxglove {

FoxgloveError::FoxgloveError(const foxglove_error&& error)
    : std::exception()
    , _impl(new foxglove_error(std::move(error))) {}

FoxgloveError::~FoxgloveError() {
  foxglove_error_free(_impl.get());
}

const char* FoxgloveError::what() const noexcept {
  return _impl->message;
}

FoxgloveErrorKind FoxgloveError::kind() const {
  switch (_impl->kind) {
    case foxglove_error_kind_ValueError:
      return FoxgloveErrorKind::ValueError;
    case foxglove_error_kind_SinkClosed:
      return FoxgloveErrorKind::SinkClosed;
    case foxglove_error_kind_SchemaRequired:
      return FoxgloveErrorKind::SchemaRequired;
    case foxglove_error_kind_MessageEncodingRequired:
      return FoxgloveErrorKind::MessageEncodingRequired;
    case foxglove_error_kind_ServerAlreadyStarted:
      return FoxgloveErrorKind::ServerAlreadyStarted;
    case foxglove_error_kind_Bind:
      return FoxgloveErrorKind::Bind;
    case foxglove_error_kind_DuplicateChannel:
      return FoxgloveErrorKind::DuplicateChannel;
    case foxglove_error_kind_DuplicateService:
      return FoxgloveErrorKind::DuplicateService;
    case foxglove_error_kind_MissingRequestEncoding:
      return FoxgloveErrorKind::MissingRequestEncoding;
    case foxglove_error_kind_ServicesNotSupported:
      return FoxgloveErrorKind::ServicesNotSupported;
    case foxglove_error_kind_ConnectionGraphNotSupported:
      return FoxgloveErrorKind::ConnectionGraphNotSupported;
    case foxglove_error_kind_IoError:
      return FoxgloveErrorKind::IoError;
    case foxglove_error_kind_McapError:
      return FoxgloveErrorKind::McapError;
    default:
      return FoxgloveErrorKind::Unspecified;
  }
}

}  // namespace foxglove
