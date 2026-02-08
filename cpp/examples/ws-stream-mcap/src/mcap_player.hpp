#pragma once

#include <foxglove/channel.hpp>
#include <foxglove/server.hpp>

#include <mcap/reader.hpp>

#include <memory>
#include <optional>
#include <string>
#include <unordered_map>

#include "playback_source.hpp"
#include "time_tracker.hpp"

/// Plays back messages from an MCAP file, implementing the PlaybackSource interface.
///
/// Uses the high-level mcap::McapReader + mcap::LinearMessageView API for iteration.
/// The LinearMessageView::Iterator supports peek-without-advance, so we only increment
/// the iterator after actually logging a message.
class McapPlayer final : public PlaybackSource {
public:
  /// Creates a new McapPlayer from the given MCAP file path.
  /// Returns nullptr on failure (prints error to stderr).
  static std::unique_ptr<McapPlayer> create(const std::string& path);

  ~McapPlayer() override;

  // PlaybackSource interface
  [[nodiscard]] std::pair<uint64_t, uint64_t> timeRange() const override;
  void setPlaybackSpeed(float speed) override;
  void play() override;
  void pause() override;
  bool seek(uint64_t log_time) override;
  [[nodiscard]] foxglove::PlaybackStatus status() const override;
  [[nodiscard]] uint64_t currentTime() const override;
  [[nodiscard]] float playbackSpeed() const override;
  std::optional<std::chrono::nanoseconds> logNextMessage(const foxglove::WebSocketServer& server
  ) override;

private:
  McapPlayer() = default;

  /// Creates RawChannels from the MCAP channel metadata.
  bool createChannels();

  /// Resets the message view iterator to start from the given log time.
  void resetMessageView(uint64_t start_time);

  mcap::McapReader reader_;
  std::unordered_map<uint16_t, foxglove::RawChannel> channels_;
  std::unique_ptr<mcap::LinearMessageView> message_view_;
  std::optional<mcap::LinearMessageView::Iterator> iterator_;
  std::optional<TimeTracker> time_tracker_;

  std::pair<uint64_t, uint64_t> time_range_ = {0, 0};
  foxglove::PlaybackStatus status_ = foxglove::PlaybackStatus::Paused;
  uint64_t current_time_ = 0;
  float playback_speed_ = 1.0F;
};
