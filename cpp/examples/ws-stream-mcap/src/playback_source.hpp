#pragma once

#include <foxglove/server.hpp>

#include <chrono>
#include <cstdint>
#include <optional>
#include <utility>

/// A data source that supports ranged playback with play/pause, seek, and variable speed.
///
/// Implementations are responsible for:
/// - Tracking playback state (playing/paused/ended) and current position
/// - Pacing message delivery according to timestamps and playback speed
/// - Logging messages to channels and broadcasting time updates to the server
class PlaybackSource {
public:
  virtual ~PlaybackSource() = default;
  PlaybackSource() = default;
  PlaybackSource(PlaybackSource&&) = default;
  PlaybackSource& operator=(PlaybackSource&&) = default;

  PlaybackSource(const PlaybackSource&) = delete;
  PlaybackSource operator=(const PlaybackSource&) = delete;

  /// Returns the (start, end) time bounds of the data in nanoseconds.
  [[nodiscard]] virtual std::pair<uint64_t, uint64_t> timeRange() const = 0;

  /// Sets the playback speed multiplier (e.g., 1.0 for real-time, 2.0 for double speed).
  virtual void setPlaybackSpeed(float speed) = 0;

  /// Begins or resumes playback.
  virtual void play() = 0;

  /// Pauses playback.
  virtual void pause() = 0;

  /// Seeks to the specified timestamp in nanoseconds. Returns true on success.
  virtual bool seek(uint64_t log_time) = 0;

  /// Returns the current playback status.
  [[nodiscard]] virtual foxglove::PlaybackStatus status() const = 0;

  /// Returns the current playback position in nanoseconds.
  [[nodiscard]] virtual uint64_t currentTime() const = 0;

  /// Returns the current playback speed multiplier.
  [[nodiscard]] virtual float playbackSpeed() const = 0;

  /// Logs the next message for playback if it's ready, or returns a duration to wait.
  virtual std::optional<std::chrono::nanoseconds> logNextMessage(
    const foxglove::WebSocketServer& server
  ) = 0;
};
