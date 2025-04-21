#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_context;

namespace foxglove {

class Context final {
  friend class McapWriter;
  friend class Channel;
  friend class WebSocketServer;

public:
  /// The default global context
  Context() = default;

  /// Create a new context
  static Context create();

private:
  explicit Context(const foxglove_context* context);

  const foxglove_context* get_inner() const {
    return _impl.get();
  }

  std::shared_ptr<const foxglove_context> _impl;
};

}  // namespace foxglove
