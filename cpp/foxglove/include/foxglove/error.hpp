
#include <exception>
#include <memory>

struct foxglove_error;

namespace foxglove {

enum class FoxgloveErrorKind {
  Unspecified,
  ValueError,
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
  McapError,
};

class FoxgloveError : public std::exception {
public:
  FoxgloveError(const foxglove_error&& error);
  ~FoxgloveError();

  virtual const char* what() const noexcept override;
  FoxgloveErrorKind kind() const;

private:
  std::unique_ptr<foxglove_error> _impl;
};

}  // namespace foxglove
