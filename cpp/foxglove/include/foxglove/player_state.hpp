#pragma once
#include <foxglove-c/foxglove-c.h>

#include <memory>
#include <optional>

namespace foxglove {

enum class PlaybackState : uint8_t {
  Playing = 0,
  Paused = 1,
  Buffering = 2,
  Ended = 3,
};

struct PlayerState {
public:
  /// @brief The playback state.
  PlaybackState playback_state;
  /// @brief The playback speed.
  float playback_speed;
  /// @brief The seek time.
  std::optional<uint64_t> seek_time;


  static PlayerState from(const foxglove_player_state& c_player_state) {
    return {
        .playback_state = static_cast<PlaybackState>(c_player_state.playback_state),
        .playback_speed =  c_player_state.playback_speed,
        .seek_time = c_player_state.seek_time
                         ? std::optional<uint64_t>(*c_player_state.seek_time)
                         : std::nullopt
    };
  }
};
}  // namespace foxglove
