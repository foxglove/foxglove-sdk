#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_context;

/// The foxglove namespace.
namespace foxglove {

/// @brief A context is the binding between channels and sinks.
///
/// Each channel and each sink belongs to exactly one context. Sinks receive advertisements about
/// channels on the context, and can optionally subscribe to receive logged messages on those
/// channels.
///
/// When the context is destroyed, its corresponding channels and sinks will be disconnected from
/// one another, and logging will stop. Attempts to log on a channel after its context has been
/// destroyed will elicit a throttled warning message.
///
/// Since many applications only need a single context, the SDK provides a static default context
/// for convenience.
///
/// @note Context is fully thread-safe.
class Context final {
public:
  /// The default global context
  Context() = default;

  /// Create a new context
  static Context create();

  /// For internal use only.
  /// @cond foxglove_internal
  [[nodiscard]] const foxglove_context* getInner() const noexcept {
    return impl_.get();
  }
  /// @endcond
private:
  explicit Context(const foxglove_context* context);

  std::shared_ptr<const foxglove_context> impl_;
};

}  // namespace foxglove
