#pragma once
#include <foxglove-c/foxglove-c.h>

#include <optional>

namespace foxglove {

/// @brief Playback state in the Foxglove app
enum class PlaybackState : uint8_t {
  /// Actively playing back
  Playing = 0,
  /// Playback paused
  Paused = 1,
  /// Player is buffering data
  Buffering = 2,
  /// Playback has ended
  Ended = 3,
};

/// @brief A request from the Foxglove app to change how the server is playing back data.
///
/// Only relevant if the `RangedPlayback` capability is enabled.
struct PlaybackControlRequest {
public:
  /// @brief The playback state.
  PlaybackState playback_state;
  /// @brief The playback speed.
  float playback_speed;
  /// @brief The requested seek time, in absolute nanoseconds. Will be std::nullopt if no seek
  /// requested.
  std::optional<uint64_t> seek_time;

  /// @brief Construct a PlaybackControlRequest from the corresponding C struct
  ///
  /// @param c_playback_control_request C struct for a playback control request
  static PlaybackControlRequest from(
    const foxglove_playback_control_request& c_playback_control_request
  ) {
    return {
      static_cast<PlaybackState>(c_playback_control_request.playback_state),
      c_playback_control_request.playback_speed,
      c_playback_control_request.seek_time
        ? std::optional<uint64_t>(*c_playback_control_request.seek_time)
        : std::nullopt
    };
  }
};
}  // namespace foxglove
