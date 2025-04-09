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
  Context();

  static Context get_default();

private:
  explicit Context(const foxglove_context* context);

  inline const foxglove_context* get_inner() const {
    return _impl.get();
  }

  std::unique_ptr<const foxglove_context, void (*)(const foxglove_context*)> _impl;
};

}  // namespace foxglove
