#pragma once

#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/expected.hpp>

#include <chrono>
#include <memory>
#include <optional>
#include <string>

namespace foxglove {

/// @brief Options for SystemInfoPublisher::create.
///
/// All fields are optional. Defaults are documented per field.
struct SystemInfoOptions final {
  /// @brief The context on which the publisher creates its channel.
  ///
  /// Defaults to the global default context.
  Context context;

  /// @brief Optional channel topic name.
  ///
  /// Defaults to `/sysinfo`.
  std::optional<std::string> topic;

  /// @brief Optional refresh interval.
  ///
  /// Defaults to 500ms. Clamped to a minimum of 200ms.
  std::optional<std::chrono::milliseconds> refresh_interval;
};

/// @brief A publisher that periodically logs process and system statistics on a channel.
///
/// The publisher creates a channel on the configured Context (defaulting to `/sysinfo`)
/// and spawns a background task that logs a `SystemInfo` message at the configured
/// interval.
///
/// The publisher runs until it is explicitly stopped via `stop()`. Destroying this
/// object does **not** stop the publisher: the background task continues running
/// until the process exits, matching the Rust and Python SDK behavior of detaching
/// the underlying task when the handle is dropped. Call `stop()` explicitly to abort
/// the background task before the process exits.
///
/// @note SystemInfoPublisher is movable but not copyable, and is thread-safe.
class SystemInfoPublisher final {
public:
  /// @brief Create and start a system info publisher with the given options.
  static FoxgloveResult<SystemInfoPublisher> create(SystemInfoOptions&& options = {});

  /// @brief Stop the publisher and free its resources.
  ///
  /// Aborts the background task. After calling stop(), the publisher is in an
  /// empty state and further calls to stop() are no-ops.
  ///
  /// This is **not** called automatically by the destructor; if you want the
  /// publisher to stop when this object goes out of scope, call stop() explicitly.
  FoxgloveError stop() noexcept;

private:
  explicit SystemInfoPublisher(foxglove_system_info_publisher* impl);

  std::unique_ptr<
    foxglove_system_info_publisher, foxglove_error (*)(foxglove_system_info_publisher*)>
    impl_;
};

}  // namespace foxglove
