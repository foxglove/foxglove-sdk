/// Remote access gateway example: demonstrates serving assets via the fetch
/// asset handler and logging scene updates that reference those assets.
///
/// A pelican STL model is read at startup and served when a client requests
/// `package://pelican/pelican.stl`. A SceneUpdate referencing the model is
/// logged every second on the `/scene` topic.
///
/// Set the FOXGLOVE_DEVICE_TOKEN environment variable before running:
///
///   FOXGLOVE_DEVICE_TOKEN=<your-token> ./example_remote_access_fetch_asset [path/to/pelican.stl]
///
/// Then open https://app.foxglove.dev and connect to the device via the
/// remote access gateway.

#include <foxglove/foxglove.hpp>
#include <foxglove/remote_access.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <fstream>
#include <functional>
#include <iostream>
#include <string>
#include <thread>
#include <unordered_map>
#include <vector>

using namespace std::chrono_literals;

static constexpr const char* kPelicanUri = "package://pelican/pelican.stl";

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

std::vector<std::byte> readFile(const std::string& path) {
  std::ifstream file(path, std::ios::binary | std::ios::ate);
  if (!file) {
    std::cerr << "Failed to open file: " << path << '\n';
    return {};
  }
  auto size = file.tellg();
  file.seekg(0);
  std::vector<std::byte> data(static_cast<size_t>(size));
  file.read(reinterpret_cast<char*>(data.data()), size);
  return data;
}

// NOLINTNEXTLINE(bugprone-exception-escape)
int main(int argc, char* argv[]) {
  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::setLogLevel(foxglove::LogLevel::Info);

  // Load the STL model from disk.
  std::string stl_path = "rust/examples/pelican.stl";
  if (argc > 1) {
    stl_path = argv[1];  // NOLINT(cppcoreguidelines-pro-bounds-pointer-arithmetic)
  }
  auto stl_data = readFile(stl_path);
  if (stl_data.empty()) {
    std::cerr << "Usage: " << argv[0]  // NOLINT(cppcoreguidelines-pro-bounds-pointer-arithmetic)
              << " [path/to/pelican.stl]\n";
    return 1;
  }
  std::cerr << "Loaded " << stl_path << " (" << stl_data.size() << " bytes)\n";

  // Build a map of URI -> asset data for the fetch asset handler.
  std::unordered_map<std::string, std::vector<std::byte>> assets;
  assets.emplace(kPelicanUri, std::move(stl_data));

  // Create the gateway with a fetch asset handler.
  foxglove::RemoteAccessGatewayOptions options;
  options.name = "remote-access-fetch-asset-cpp";
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

  options.fetch_asset = [&assets](std::string_view uri, foxglove::FetchAssetResponder&& responder) {
    auto it = assets.find(std::string(uri));
    if (it != assets.end()) {
      std::cerr << "Serving asset: " << uri << " (" << it->second.size() << " bytes)\n";
      std::move(responder).respondOk(it->second);
    } else {
      std::cerr << "Asset not found: " << uri << '\n';
      std::string msg = "Asset not found: ";
      msg += uri;
      std::move(responder).respondError(msg);
    }
  };

  auto gateway_result = foxglove::RemoteAccessGateway::create(std::move(options));
  if (!gateway_result.has_value()) {
    std::cerr << "Failed to create gateway: " << foxglove::strerror(gateway_result.error()) << '\n';
    std::cerr << "Set the FOXGLOVE_DEVICE_TOKEN environment variable.\n";
    return 1;
  }
  auto gateway = std::move(gateway_result.value());

  // Create a channel for SceneUpdate messages.
  auto scene_channel_result = foxglove::messages::SceneUpdateChannel::create("/scene");
  if (!scene_channel_result.has_value()) {
    std::cerr << "Failed to create scene channel\n";
    return 1;
  }
  auto scene_channel = std::move(scene_channel_result.value());

  // Build a SceneUpdate with a pelican model.
  foxglove::messages::ModelPrimitive model;
  model.url = kPelicanUri;
  model.media_type = "model/stl";
  model.pose = foxglove::messages::Pose{
    foxglove::messages::Vector3{0.0, 0.0, 0.0},
    foxglove::messages::Quaternion{0.0, 0.0, 0.0, 1.0},
  };
  model.scale = foxglove::messages::Vector3{0.01, 0.01, 0.01};
  model.color = foxglove::messages::Color{0.8, 0.6, 0.2, 1.0};

  foxglove::messages::SceneEntity entity;
  entity.frame_id = "world";
  entity.id = "pelican";
  entity.models.push_back(std::move(model));

  foxglove::messages::SceneUpdate scene;
  scene.entities.push_back(std::move(entity));

  // Set up signal handler for graceful shutdown.
  std::atomic_bool done = false;
  sigint_handler = [&] {
    std::cerr << "Shutting down...\n";
    done = true;
  };

  // Publish scene updates at 1 Hz.
  while (!done) {
    scene_channel.log(scene);
    std::this_thread::sleep_for(1s);
  }

  gateway.stop();
  std::cerr << "Done\n";
  return 0;
}
