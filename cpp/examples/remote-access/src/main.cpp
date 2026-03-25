#include <foxglove/channel.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/remote_access.hpp>

#include <atomic>
#include <chrono>
#include <cmath>
#include <csignal>
#include <cstring>
#include <functional>
#include <iostream>
#include <thread>
#include <vector>

using namespace std::chrono_literals;

constexpr uint32_t WIDTH = 480;
constexpr uint32_t HEIGHT = 270;
constexpr uint32_t BYTES_PER_PIXEL = 3;
constexpr uint32_t STEP = WIDTH * BYTES_PER_PIXEL;
constexpr double FX = 250.0;
constexpr double FY = 250.0;
constexpr double CX = WIDTH / 2.0;
constexpr double CY = HEIGHT / 2.0;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

/// Fill the image buffer with a scrolling vertical color ramp.
/// Each column is a single hue that shifts over time, producing smooth horizontal motion.
void renderColorRamp(std::vector<uint8_t>& buf, uint64_t frame) {
  for (uint32_t y = 0; y < HEIGHT; ++y) {
    for (uint32_t x = 0; x < WIDTH; ++x) {
      // Hue shifts with x position and frame number
      double hue = std::fmod(static_cast<double>(x + frame * 2) / WIDTH * 360.0, 360.0);
      // Brightness varies with y
      double brightness = static_cast<double>(y) / HEIGHT;

      // HSV to RGB (S=1, V=brightness)
      double c = brightness;
      double h = hue / 60.0;
      double frac = h - std::floor(h);
      auto v = static_cast<uint8_t>(c * 255);
      auto p = static_cast<uint8_t>(0);
      auto q = static_cast<uint8_t>(c * (1.0 - frac) * 255);
      auto t = static_cast<uint8_t>(c * frac * 255);

      uint8_t r = 0;
      uint8_t g = 0;
      uint8_t b = 0;
      int sector = static_cast<int>(h) % 6;
      switch (sector) {
        case 0:
          r = v;
          g = t;
          b = p;
          break;
        case 1:
          r = q;
          g = v;
          b = p;
          break;
        case 2:
          r = p;
          g = v;
          b = t;
          break;
        case 3:
          r = p;
          g = q;
          b = v;
          break;
        case 4:
          r = t;
          g = p;
          b = v;
          break;
        case 5:
          r = v;
          g = p;
          b = q;
          break;
      }

      size_t off = static_cast<size_t>(y) * STEP + static_cast<size_t>(x) * BYTES_PER_PIXEL;
      buf[off] = r;
      buf[off + 1] = g;
      buf[off + 2] = b;
    }
  }
}

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::setLogLevel(foxglove::LogLevel::Info);

  // Create the remote access gateway.
  foxglove::RemoteAccessGatewayOptions options = {};
  options.name = "remote-access-example-cpp";
  options.callbacks.onConnectionStatusChanged = [](foxglove::RemoteAccessConnectionStatus status) {
    const char* label = "unknown";
    switch (status) {
      case foxglove::RemoteAccessConnectionStatus::Connecting:
        label = "connecting";
        break;
      case foxglove::RemoteAccessConnectionStatus::Connected:
        label = "connected";
        break;
      case foxglove::RemoteAccessConnectionStatus::ShuttingDown:
        label = "shutting down";
        break;
      case foxglove::RemoteAccessConnectionStatus::Shutdown:
        label = "shutdown";
        break;
    }
    std::cerr << "Connection status: " << label << '\n';
  };

  auto gateway_result = foxglove::RemoteAccessGateway::create(std::move(options));
  if (!gateway_result.has_value()) {
    std::cerr << "Failed to create gateway: " << foxglove::strerror(gateway_result.error()) << '\n';
    std::cerr << "Set the FOXGLOVE_DEVICE_TOKEN environment variable.\n";
    return 1;
  }
  auto gateway = std::move(gateway_result.value());

  // Create channels.
  auto image_channel_result = foxglove::messages::RawImageChannel::create("/camera/image");
  if (!image_channel_result.has_value()) {
    std::cerr << "Failed to create image channel\n";
    return 1;
  }
  auto image_channel = std::move(image_channel_result.value());

  auto cal_channel_result =
    foxglove::messages::CameraCalibrationChannel::create("/camera/calibration");
  if (!cal_channel_result.has_value()) {
    std::cerr << "Failed to create calibration channel\n";
    return 1;
  }
  auto cal_channel = std::move(cal_channel_result.value());

  // Set up signal handler for graceful shutdown.
  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    done = true;
  };

  // Build a static camera calibration message.
  foxglove::messages::CameraCalibration calibration;
  calibration.frame_id = "camera";
  calibration.width = WIDTH;
  calibration.height = HEIGHT;
  calibration.k = {FX, 0, CX, 0, FY, CY, 0, 0, 1};
  calibration.p = {FX, 0, CX, 0, 0, FY, CY, 0, 0, 0, 1, 0};

  // Publish loop.
  std::vector<uint8_t> image_buf(static_cast<size_t>(WIDTH) * HEIGHT * BYTES_PER_PIXEL);
  uint64_t frame = 0;
  while (!done) {
    std::this_thread::sleep_for(33ms);  // ~30 fps

    auto now =
      static_cast<uint64_t>(std::chrono::system_clock::now().time_since_epoch().count()) / 1000;

    renderColorRamp(image_buf, frame);

    foxglove::messages::RawImage image;
    image.frame_id = "camera";
    image.width = WIDTH;
    image.height = HEIGHT;
    image.encoding = "rgb8";
    image.step = STEP;
    image.data.assign(
      reinterpret_cast<const std::byte*>(image_buf.data()),
      reinterpret_cast<const std::byte*>(image_buf.data() + image_buf.size())
    );
    image_channel.log(image, now);

    cal_channel.log(calibration, now);

    ++frame;
  }

  gateway.stop();
  std::cerr << "Done\n";
  return 0;
}
