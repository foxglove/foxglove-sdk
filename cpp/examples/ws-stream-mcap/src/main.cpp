#include <foxglove/foxglove.hpp>
#include <foxglove/server.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <memory>
#include <mutex>
#include <string>
#include <thread>

#include "mcap_player.hpp"

using namespace std::chrono_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

static void printUsage(const char* program) {
  std::cerr << "Usage: " << program << " --file <path> [--port <num>] [--host <addr>]\n"
            << "  --file <path>   MCAP file to stream (required)\n"
            << "  --port <num>    Server port (default: 8765)\n"
            << "  --host <addr>   Server host (default: 127.0.0.1)\n";
}

// NOLINTNEXTLINE(bugprone-exception-escape)
int main(int argc, char* argv[]) {
  std::string file_path;
  uint16_t port = 8765;
  std::string host = "127.0.0.1";

  // Parse CLI arguments
  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if ((arg == "--file" || arg == "-f") && i + 1 < argc) {
      file_path = argv[++i];
    } else if ((arg == "--port" || arg == "-p") && i + 1 < argc) {
      port = static_cast<uint16_t>(std::stoi(argv[++i]));
    } else if (arg == "--host" && i + 1 < argc) {
      host = argv[++i];
    } else if (arg == "--help" || arg == "-h") {
      printUsage(argv[0]);
      return 0;
    } else {
      std::cerr << "Unknown argument: " << arg << '\n';
      printUsage(argv[0]);
      return 1;
    }
  }

  if (file_path.empty()) {
    std::cerr << "Error: --file is required\n";
    printUsage(argv[0]);
    return 1;
  }

  foxglove::setLogLevel(foxglove::LogLevel::Info);

  // Extract file name for the server name
  std::string server_name = file_path;
  auto slash_pos = server_name.find_last_of('/');
  if (slash_pos != std::string::npos) {
    server_name = server_name.substr(slash_pos + 1);
  }

  std::cerr << "Loading MCAP summary\n";

  auto player = McapPlayer::create(file_path);
  if (!player) {
    return 1;
  }

  auto time_range = player->timeRange();
  auto shared_player = std::make_shared<std::mutex>();
  auto player_ptr = std::shared_ptr<McapPlayer>(std::move(player));

  foxglove::WebSocketServerOptions options = {};
  options.name = server_name;
  options.host = host;
  options.port = port;
  options.capabilities = foxglove::WebSocketServerCapabilities::RangedPlayback |
                         foxglove::WebSocketServerCapabilities::Time;
  options.playback_time_range = time_range;

  const auto& mtx = shared_player;
  const auto& player_ref = player_ptr;
  options.callbacks.onPlaybackControlRequest = [mtx, player_ref](
                                                 const foxglove::PlaybackControlRequest& request
                                               ) -> std::optional<foxglove::PlaybackState> {
    std::lock_guard<std::mutex> lock(*mtx);

    bool did_seek = request.seek_time.has_value();

    if (request.seek_time.has_value()) {
      if (!player_ref->seek(*request.seek_time)) {
        did_seek = false;
      }
    }

    player_ref->setPlaybackSpeed(request.playback_speed);

    switch (request.playback_command) {
      case foxglove::PlaybackCommand::Play:
        player_ref->play();
        break;
      case foxglove::PlaybackCommand::Pause:
        player_ref->pause();
        break;
    }

    return foxglove::PlaybackState{
      player_ref->status(),
      player_ref->currentTime(),
      player_ref->playbackSpeed(),
      did_seek,
      request.request_id,
    };
  };

  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error()) << '\n';
    return 1;
  }
  auto server = std::move(server_result.value());

  std::atomic_bool done{false};
  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    done = true;
  };

  std::cerr << "Server listening on " << host << ":" << port << '\n';
  std::cerr << "Waiting for client\n";
  std::this_thread::sleep_for(1s);

  std::cerr << "Starting stream\n";
  auto last_status = foxglove::PlaybackStatus::Paused;
  foxglove::PlaybackStatus current_status = foxglove::PlaybackStatus::Paused;

  while (!done) {
    {
      std::lock_guard<std::mutex> lock(*mtx);
      current_status = player_ptr->status();

      if (current_status == foxglove::PlaybackStatus::Ended &&
          last_status != foxglove::PlaybackStatus::Ended) {
        server.broadcastPlaybackState(foxglove::PlaybackState{
          foxglove::PlaybackStatus::Ended,
          player_ptr->currentTime(),
          player_ptr->playbackSpeed(),
          false,
          std::nullopt,
        });
      }
    }
    last_status = current_status;

    if (current_status != foxglove::PlaybackStatus::Playing) {
      std::this_thread::sleep_for(10ms);
      continue;
    }

    std::optional<std::chrono::nanoseconds> sleep_duration;
    {
      std::lock_guard<std::mutex> lock(*mtx);
      sleep_duration = player_ptr->logNextMessage(server);
    }

    if (sleep_duration.has_value()) {
      auto capped = std::min(*sleep_duration, std::chrono::nanoseconds(1'000'000'000));
      std::this_thread::sleep_for(capped);
    }
  }

  server.stop();
  return 0;
}
