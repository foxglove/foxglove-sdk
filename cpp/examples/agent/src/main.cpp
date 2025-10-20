#include <foxglove/channel.hpp>
#include <foxglove/cloud_sink.hpp>
#include <foxglove/foxglove.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <memory>
#include <thread>

using namespace std::chrono_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

/// Produce example image data (a gradient). Offset can be used to 'animate' the gradient.
std::vector<std::byte> gradient_data(size_t width, size_t height, size_t offset) {
  std::vector<std::byte> data(width * height * 3);
  for (size_t y = 0; y < height; ++y) {
    for (size_t x = 0; x < width; ++x) {
      size_t idx = (y * width + x) * 3;
      size_t shifted_x = (x + offset) % width;
      auto gradient = static_cast<uint8_t>(shifted_x * 255 / width);

      // B, G, R
      data[idx] = static_cast<std::byte>(gradient);
      data[idx + 1] = static_cast<std::byte>(255 - gradient);
      data[idx + 2] = static_cast<std::byte>(gradient / 2);
    }
  }
  return data;
}

void camera_loop(std::atomic_bool& done, foxglove::schemas::RawImageChannel& channel) {
  size_t offset = 0;
  uint32_t width = 960;
  uint32_t height = 540;

  while (!done) {
    foxglove::schemas::RawImage image;
    image.width = width;
    image.height = height;
    image.encoding = "rgb8";
    image.step = width * 3;
    image.data = gradient_data(width, height, offset);
    channel.log(image);

    std::this_thread::sleep_for(33ms);

    offset = (offset + 1) % width;
  }
}

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  std::map<std::uint32_t, std::string> topic_map;

  foxglove::CloudSinkOptions options = {};
  options.supported_encodings = {"json"};
  options.callbacks.onClientAdvertise =
    [&topic_map]([[maybe_unused]] uint32_t client_id, const foxglove::ClientChannel& channel) {
      topic_map[channel.id] = channel.topic;
    };
  options.callbacks.onMessageData =
    [&topic_map](
      uint32_t client_id, uint32_t client_channel_id, const std::byte* data, size_t data_len
    ) {
      ;
      if (auto result = topic_map.find(client_channel_id); result != topic_map.end()) {
        auto topic = result->second;
        std::cerr << "Teleop message from: " << client_id << " on topic " << topic << ": "
                  << std::string(reinterpret_cast<const char*>(data), data_len) << '\n';
      }
    };

  auto server_result = foxglove::CloudSink::create(std::move(options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error()) << '\n';
    return 1;
  }
  auto server = std::move(server_result.value());

  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    server.stop();
    done = true;
  };

  auto channel_result = foxglove::schemas::RawImageChannel::create("/camera");
  if (!channel_result.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto channel = std::move(channel_result.value());

  camera_loop(done, channel);

  return 0;
}
