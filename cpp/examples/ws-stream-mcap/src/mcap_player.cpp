#define MCAP_IMPLEMENTATION
#include "mcap_player.hpp"

#include <foxglove/schema.hpp>

#include <iostream>
#include <string>

std::unique_ptr<McapPlayer> McapPlayer::create(const std::string& path) {
  auto player = std::unique_ptr<McapPlayer>(new McapPlayer());

  auto status = player->reader_.open(path);
  if (!status.ok()) {
    std::cerr << "Failed to open MCAP file: " << status.message << '\n';
    return nullptr;
  }

  auto on_problem = [](const mcap::Status& problem) {
    std::cerr << "MCAP read problem: " << problem.message << '\n';
  };

  status = player->reader_.readSummary(mcap::ReadSummaryMethod::AllowFallbackScan, on_problem);
  if (!status.ok()) {
    std::cerr << "Failed to read MCAP summary: " << status.message << '\n';
    return nullptr;
  }

  // Extract time range from statistics
  auto stats = player->reader_.statistics();
  if (!stats.has_value()) {
    std::cerr << "MCAP file has no statistics record\n";
    return nullptr;
  }
  player->time_range_ = {stats->messageStartTime, stats->messageEndTime};
  player->current_time_ = stats->messageStartTime;

  if (!player->createChannels()) {
    return nullptr;
  }

  player->resetMessageView(player->current_time_);

  return player;
}

McapPlayer::~McapPlayer() {
  iterator_.reset();
  message_view_.reset();
  reader_.close();
}

bool McapPlayer::createChannels() {
  const auto& schemas = reader_.schemas();
  for (const auto& [id, channel_ptr] : reader_.channels()) {
    std::optional<foxglove::Schema> schema;
    if (channel_ptr->schemaId != 0) {
      auto schema_it = schemas.find(channel_ptr->schemaId);
      if (schema_it != schemas.end()) {
        const auto& mcap_schema = schema_it->second;
        foxglove::Schema s;
        s.name = mcap_schema->name;
        s.encoding = mcap_schema->encoding;
        s.data = reinterpret_cast<const std::byte*>(mcap_schema->data.data());
        s.data_len = mcap_schema->data.size();
        schema = std::move(s);
      }
    }

    auto channel_result = foxglove::RawChannel::create(
      channel_ptr->topic, channel_ptr->messageEncoding, std::move(schema)
    );
    if (!channel_result.has_value()) {
      std::cerr << "Failed to create channel for topic '" << channel_ptr->topic
                << "': " << foxglove::strerror(channel_result.error()) << '\n';
      return false;
    }
    channels_.emplace(id, std::move(channel_result.value()));
  }
  return true;
}

void McapPlayer::resetMessageView(uint64_t start_time) {
  iterator_.reset();
  message_view_.reset();

  mcap::ReadMessageOptions opts;
  opts.startTime = start_time;
  opts.endTime = time_range_.second + 1;

  message_view_ = std::make_unique<mcap::LinearMessageView>(reader_.readMessages(
    [](const mcap::Status& problem) {
      std::cerr << "MCAP message read problem: " << problem.message << '\n';
    },
    opts
  ));
  iterator_ = message_view_->begin();

  time_tracker_.reset();
}

std::pair<uint64_t, uint64_t> McapPlayer::timeRange() const {
  return time_range_;
}

void McapPlayer::setPlaybackSpeed(float speed) {
  speed = TimeTracker::clampSpeed(speed);
  if (time_tracker_.has_value()) {
    time_tracker_->setSpeed(speed);
  }
  playback_speed_ = speed;
}

void McapPlayer::play() {
  if (status_ == foxglove::PlaybackStatus::Ended) {
    return;
  }
  if (time_tracker_.has_value()) {
    time_tracker_->resume();
  }
  status_ = foxglove::PlaybackStatus::Playing;
}

void McapPlayer::pause() {
  if (time_tracker_.has_value()) {
    time_tracker_->pause();
  }
  status_ = foxglove::PlaybackStatus::Paused;
}

bool McapPlayer::seek(uint64_t log_time) {
  log_time = std::max(time_range_.first, std::min(log_time, time_range_.second));
  resetMessageView(log_time);
  current_time_ = log_time;
  if (status_ == foxglove::PlaybackStatus::Ended) {
    status_ = foxglove::PlaybackStatus::Paused;
  }
  return true;
}

foxglove::PlaybackStatus McapPlayer::status() const {
  return status_;
}

uint64_t McapPlayer::currentTime() const {
  return current_time_;
}

float McapPlayer::playbackSpeed() const {
  return playback_speed_;
}

std::optional<std::chrono::nanoseconds> McapPlayer::logNextMessage(
  const foxglove::WebSocketServer& server
) {
  if (status_ != foxglove::PlaybackStatus::Playing) {
    return std::nullopt;
  }

  if (!iterator_.has_value() || !message_view_ || *iterator_ == message_view_->end()) {
    status_ = foxglove::PlaybackStatus::Ended;
    current_time_ = time_range_.second;
    return std::nullopt;
  }

  const auto& msg = **iterator_;

  // Initialize the time tracker on the first message
  if (!time_tracker_.has_value()) {
    time_tracker_.emplace(msg.message.logTime, playback_speed_);
  }

  auto wakeup = time_tracker_->wakeupFor(msg.message.logTime);
  auto now = std::chrono::steady_clock::now();
  if (wakeup > now) {
    auto sleep_duration = std::chrono::duration_cast<std::chrono::nanoseconds>(wakeup - now);
    return sleep_duration;
  }

  current_time_ = msg.message.logTime;

  if (auto timestamp = time_tracker_->notify(msg.message.logTime)) {
    // Broadcast time with the current playback time (nanoseconds since epoch).
    // Requires WebSocketServerCapabilities::Time to be advertised by the server.
    server.broadcastTime(*timestamp);
  }

  auto channel_it = channels_.find(static_cast<uint16_t>(msg.message.channelId));
  if (channel_it != channels_.end()) {
    channel_it->second.log(
      reinterpret_cast<const std::byte*>(msg.message.data),
      msg.message.dataSize,
      msg.message.logTime
    );
  }

  ++(*iterator_);
  return std::nullopt;
}
