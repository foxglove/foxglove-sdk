#include <foxglove/channel.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/server.hpp>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <csignal>
#include <cstdint>
#include <functional>
#include <iostream>
#include <mutex>
#include <thread>

using namespace std::chrono_literals;

// Example class for playing back a fixed interval of robot stack data.
//
// More practical implementations would load data from a file on disk, but for the sake of
// illustration, this example generates a fixed buffer of example data in-memory.
class DataPlayer {
public:
  DataPlayer(size_t num_timesteps, foxglove::RawChannel&& channel)
      : channel_(std::move(channel)) {
    // Generate a buffer of example data, in this case a sine wave sampled at 1Hz. The first
    // element of the pair is the timestamp in absoute nanoseconds, and the second is the data
    // field.
    data_.reserve(num_timesteps);
    for (uint64_t t = 0; t < num_timesteps; ++t) {
      data_.emplace_back(t * 1000000000ULL, std::sin(static_cast<double>(t)));
    }
  }

  ~DataPlayer() {
    if (!server_) {
      return;
    }

    server_->stop();
  }

  // Timestamps, in nanoseconds, defining the bounds of data that we can play back. This is used by
  // the Foxglove player to set up the time bar for scrubbing in its UI.
  std::pair<uint64_t, uint64_t> playbackTimeRange() {
    return {data_.front().first, data_.back().first};
  }

  void startServer() {
    foxglove::WebSocketServerOptions options = {};
    options.name = "mcap-ws-demo-cpp";
    options.host = "127.0.0.1";
    options.port = 8765;

    // To enable playback controls and seeking in the Foxglove player, the server must declare the
    // time range of data it is playing back and declare both the `RangedPlayback` and `Time`
    // capabilities.
    options.playback_time_range = playbackTimeRange();
    options.capabilities =
      (foxglove::WebSocketServerCapabilities::RangedPlayback |
       foxglove::WebSocketServerCapabilities::Time);

    options.supported_encodings = {"json"};
    options.callbacks.onSubscribe =
      [](uint64_t channel_id, const foxglove::ClientMetadata& client) {
        std::cerr << "Client " << client.id << " subscribed to channel " << channel_id << '\n';
      };
    options.callbacks.onUnsubscribe =
      [](uint64_t channel_id, const foxglove::ClientMetadata& client) {
        std::cerr << "Client " << client.id << " unsubscribed from channel " << channel_id << '\n';
      };
    options.callbacks.onPlaybackControlRequest = [this](
                                                   const foxglove::PlaybackControlRequest& request
                                                 ) -> std::optional<foxglove::PlaybackState> {
      return this->onPlaybackControlRequest(request);
    };

    auto server_result = foxglove::WebSocketServer::create(std::move(options));
    if (!server_result.has_value()) {
      throw std::runtime_error(
        std::string("Failed to create server: ") +
        std::string(foxglove::strerror(server_result.error()))
      );
    }

    server_ = std::make_unique<foxglove::WebSocketServer>(std::move(server_result.value()));
  }

  static std::string toMessage(const std::pair<uint64_t, double>& data) {
    return "{\"val\": " + std::to_string(data.second) + "}";
  }

  void tick() {
    if (!server_) {
      throw std::runtime_error("Tried to tick with uninitialized server");
    }

    if (data_.empty()) {
      throw std::runtime_error("Tried to tick with empty data");
    }

    if (currentPlaybackState().status == foxglove::PlaybackStatus::Paused) {
      std::this_thread::sleep_for(50ms);
      return;
    }

    float playback_speed = 1.0;
    {
      std::lock_guard<std::mutex> lock(playback_mutex_);
      playback_speed = playback_speed_;

      // Playback requires the server to broadcast its understanding of the current time to advance
      // time forward in the Foxglove player
      server_->broadcastTime(current_time_);

      // Create a JSON payload containing the data message and log to the channel. This will cause
      // the data to be sent to Foxglove over the WebSocket.
      std::string msg = toMessage(data_[current_playback_index_]);
      channel_.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size(), current_time_);

      // After publishing the message, update time and playback state
      ++current_playback_index_;
      if (current_playback_index_ == data_.size()) {
        current_playback_index_ = 0;
        current_time_ = data_.front().first;
        playing_ = false;

        // If playback is over, communicate that to the Foxglove player, by emitting a PlaybackState
        // with its status set to PlaybackStatus::Ended. For our own convenience, we then reset the
        // current time and playback index to the start of the data buffer, and enter a Paused
        // state.
        server_->broadcastPlaybackState(
          {foxglove::PlaybackStatus::Ended, current_time_, playback_speed_, false, std::nullopt}
        );
        return;
      }
      current_time_ = data_[current_playback_index_].first;
    }

    std::this_thread::sleep_for(1000ms / std::max<double>(playback_speed, 0.1));
  }

  // Handler for PlaybackControlRequest messages sent from the Foxglove player. This requires
  // returning the current state of playback in the form of a PlaybackState.
  //
  // NOTE: While the PlaybackState message has a field for `request_id`, setting it explicitly from
  // within this handler has no effect; it is overwritten to match the `request_id` field in the
  // input PlaybackControlRequest.
  std::optional<foxglove::PlaybackState> onPlaybackControlRequest(
    const foxglove::PlaybackControlRequest& request
  ) {
    std::lock_guard<std::mutex> lock(playback_mutex_);

    switch (request.playback_command) {
      case foxglove::PlaybackCommand::Play:
        playing_ = true;
        break;
      case foxglove::PlaybackCommand::Pause:
        playing_ = false;
        break;
    }

    playback_speed_ = request.playback_speed;

    if (request.seek_time.has_value()) {
      seekInternal(*request.seek_time);
    }

    auto playback_state = currentPlaybackStateInternal();
    if (request.seek_time.has_value()) {
      playback_state.did_seek = true;
    }

    return playback_state;
  }

  void seek(uint64_t seek_time) {
    std::lock_guard<std::mutex> lock(playback_mutex_);
    seekInternal(seek_time);
  }

  foxglove::PlaybackState currentPlaybackState() {
    std::lock_guard<std::mutex> lock(playback_mutex_);
    return currentPlaybackStateInternal();
  };

  DataPlayer(const DataPlayer&) = delete;
  DataPlayer(const DataPlayer&&) = delete;
  DataPlayer& operator=(const DataPlayer&) = delete;
  DataPlayer&& operator=(const DataPlayer&&) = delete;

private:
  // Sets the current playback state to the given seek_time; assumes that the playback_mutex_ is
  // locked.
  void seekInternal(uint64_t seek_time) {
    if (data_.empty()) {
      current_playback_index_ = 0;
      current_time_ = 0;
      return;
    }

    auto it =
      std::lower_bound(data_.begin(), data_.end(), seek_time, [](const auto& entry, uint64_t time) {
        return entry.first < time;
      });

    // If we didn't find an exact match, rewind to the message immediately before the seek_time
    // (or clamp to the first/last entry as needed).
    if (it == data_.end()) {
      it = std::prev(data_.end());
    } else if (it->first > seek_time && it != data_.begin()) {
      --it;
    }

    current_playback_index_ = static_cast<size_t>(std::distance(data_.begin(), it));
    current_time_ = it->first;
  }

  // Gets the current playback state; assumes that the playback_mutex_ is locked.
  [[nodiscard]] foxglove::PlaybackState currentPlaybackStateInternal() const {
    return {
      playing_ ? foxglove::PlaybackStatus::Playing : foxglove::PlaybackStatus::Paused,
      current_time_,
      playback_speed_,
      false,
      std::nullopt,
    };
  }

  std::vector<std::pair<uint64_t, double>> data_;

  foxglove::RawChannel channel_;
  std::unique_ptr<foxglove::WebSocketServer> server_ = nullptr;

  // Internal variables for orchestrating playback. In addition to accessing data_, these are are
  // used to generate a foxglove::PlaybackState to send to the Foxglove player.
  // Access to these variables is protected by playback_mutex_.
  size_t current_playback_index_ = 0;
  bool playing_ = false;
  uint64_t current_time_ = 0;
  float playback_speed_ = 1.0F;
  std::mutex playback_mutex_;
};

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  // Setup output channel
  foxglove::Schema schema;
  schema.name = "float";
  schema.encoding = "jsonschema";
  std::string schema_data = R"({
    "type": "object",
    "properties": {
      "val": { "type": "number" }
    }
  })";
  schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  schema.data_len = schema_data.size();
  auto channel_result = foxglove::RawChannel::create("example", "json", std::move(schema));
  if (!channel_result.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channel_result.value());

  constexpr size_t num_time_steps = 100;
  DataPlayer player(num_time_steps, std::move(channel));

  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    done = true;
  };

  player.startServer();

  while (!done) {
    player.tick();
  }

  std::cerr << "Done\n";
  return 0;
}
