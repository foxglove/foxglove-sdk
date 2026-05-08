#include <iostream>
#include <sstream>

#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>

namespace foxglove {

const char* strerror(FoxgloveError error) {
  return foxglove_error_to_cstr(static_cast<foxglove_error>(error));
}

struct WarnStream::Impl {
  std::ostringstream buffer;
};

WarnStream::WarnStream()
    : impl_(std::make_unique<Impl>()) {}

WarnStream::~WarnStream() {
#ifndef FOXGLOVE_DISABLE_CPP_WARNINGS
  auto msg = impl_->buffer.str();
  if (!msg.empty()) {
    std::cerr << "[foxglove] " << msg << "\n";
  }
#endif
}

std::ostream& WarnStream::stream() {
  return impl_->buffer;
}

}  // namespace foxglove
