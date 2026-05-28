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

// Keep this comment in sync with rust/foxglove/src/system_info.rs

/// @brief A publisher that periodically logs process and system statistics on a channel.
///
/// The publisher creates a channel on the configured Context (defaulting to `/sysinfo`)
/// and spawns a background task that logs a `SystemInfo` message at the configured
/// interval with stats on CPU and memory usage for the process and the system.
///
/// @par Published metrics
///
/// Each message is a JSON object with a JSON Schema attached to the channel.
/// The following fields are published:
///
/// - `process_memory` (number): Resident memory used by the SDK process, in bytes.
/// - `process_virtual_memory` (number): Virtual memory used by the SDK process, in bytes.
/// - `process_cpu_percent` (number): CPU usage for the SDK process, as a percent of total
///   system CPU capacity (0.0 to 100.0).
/// - `process_cpu_cores` (number): CPU usage for the SDK process, expressed in
///   core-equivalents (0.0 to `num_cpus`). 1.0 means a single logical CPU is fully utilized.
/// - `total_cpu_percent` (number): Total CPU usage across all logical CPUs on the system,
///   as a percent (0.0 to 100.0).
/// - `total_cpu_cores` (number): Total CPU usage across the system, expressed in
///   core-equivalents (0.0 to `num_cpus`). 1.0 means one logical CPU's worth of work is being
///   done.
/// - `num_cpus` (integer): Number of logical CPUs on the system.
/// - `total_memory` (number): Total physical memory on the system, in bytes.
/// - `used_memory` (number): Used physical memory on the system, in bytes.
/// - `total_swap` (number): Total swap space on the system, in bytes.
/// - `used_swap` (number): Used swap space on the system, in bytes.
/// - `kernel_version` (string): Kernel version string, or empty if unknown.
/// - `os_version` (string): OS version string, or empty if unknown.
///
/// CPU usage values are computed from the difference between consecutive samples, so they
/// reflect activity over the most recent refresh interval.
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
