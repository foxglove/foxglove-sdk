#pragma once

#include <foxglove-c/foxglove-c.h>

#include <optional>
#include <string>

namespace foxglove {
/// @cond foxglove_internal

/// @brief The status of server data playback
enum class PlaybackStatus : uint8_t {
  /// Playing at the requested playback speed
  Playing = 0,
  /// Playback paused
  Paused = 1,
  /// Server is not yet playing back data because it is performing a prerequisite required operation
  Buffering = 2,
  /// The end of the available data has been reached
  Ended = 3,
};

/// @brief The state of the server playing back data.
///
/// Should be sent in response to a PlaybackControlRequest, or any time the
/// state of playback has changed; for example, reaching the end of data, or an external mechanism
/// causes playback to pause.
///
/// Only relevant if the `RangedPlayback` capability is enabled.
struct PlaybackState {
public:
  /// @brief The status of server data playback
  PlaybackStatus status;
  /// @brief The current time of playback, in absolute nanoseconds
  uint64_t current_time;
  /// @brief The speed of playback, as a factor of realtime
  float playback_speed;
  /// @brief Whether a seek forward or backward in time triggered this message to be emitted
  bool did_seek;
  /// @brief If this message is being emitted in response to a PlaybackControlRequest message, the
  /// request_id from that message. Set this to std::nullopt if the state of playback has been
  /// changed by any other condition.
  std::optional<std::string> request_id;
};

/// @endcond
}  // namespace foxglove
