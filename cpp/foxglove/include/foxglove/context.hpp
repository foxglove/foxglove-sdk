#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_context;

namespace foxglove {

class Context final {
public:
  /// The default global context
  Context() = default;

  /// Create a new context
  static Context create();

  /// For internal use only.
  [[nodiscard]] const foxglove_context* getInner() const {
    return impl_.get();
  }

private:
  explicit Context(const foxglove_context* context);

  std::shared_ptr<const foxglove_context> impl_;
};

}  // namespace foxglove
