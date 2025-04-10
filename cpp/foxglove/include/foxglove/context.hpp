#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_context;

namespace foxglove {

typedef foxglove_context ContextInner;

class Context final {
  friend class McapWriterOptions;
  friend class Channel;
  friend class WebSocketServerOptions;

public:
  Context();

  static Context get_default();

private:
  explicit Context(const foxglove_context* context);

  inline const ContextInner* get_inner() const {
    return _impl.get();
  }

  std::unique_ptr<const foxglove_context, void (*)(const foxglove_context*)> _impl;
};

}  // namespace foxglove
