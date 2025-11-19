#pragma once
#include <foxglove-c/foxglove-c.h>

#include <optional>
#include <string>

namespace foxglove {
/// @cond foxglove_internal

/// @brief Playback command coming from the Foxglove app
enum class PlaybackCommand : uint8_t {
  /// Start or continue playback
  Play = 0,
  /// Pause playback
  Pause = 1,
};

/// @brief A request to control playback from the Foxglove app
///
/// Only relevant if the `RangedPlayback` capability is enabled.
struct PlaybackControlRequest {
public:
  /// @brief The playback command.
  PlaybackCommand playback_command;
  /// @brief The playback speed.
  float playback_speed;
  /// @brief The requested seek time, in absolute nanoseconds. Will be std::nullopt if no seek
  /// requested.
  std::optional<uint64_t> seek_time;
  /// @brief The request ID.
  std::string request_id;

  /// @brief Construct a PlaybackControlRequest from the corresponding C struct
  ///
  /// @param c_playback_control_request C struct for a playback control request
  static PlaybackControlRequest from(
    const foxglove_playback_control_request& c_playback_control_request
  ) {
    return {
      static_cast<PlaybackCommand>(c_playback_control_request.playback_command),
      c_playback_control_request.playback_speed,
      c_playback_control_request.seek_time != nullptr
        ? std::optional<uint64_t>(*c_playback_control_request.seek_time)
        : std::nullopt,
      std::string(
        c_playback_control_request.request_id.data, c_playback_control_request.request_id.len
      )
    };
  }
};
/// @endcond
}  // namespace foxglove
