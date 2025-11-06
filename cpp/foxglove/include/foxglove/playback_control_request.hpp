#pragma once
#include <foxglove-c/foxglove-c.h>

#include <optional>

namespace foxglove {

enum class PlaybackState : uint8_t {
  Playing = 0,
  Paused = 1,
  Buffering = 2,
  Ended = 3,
};

struct PlaybackControlRequest {
public:
  /// @brief The playback state.
  PlaybackState playback_state;
  /// @brief The playback speed.
  float playback_speed;
  /// @brief The seek time.
  std::optional<uint64_t> seek_time;

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
