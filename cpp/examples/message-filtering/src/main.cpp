/**
 * This example demonstrates how to use the Foxglove SDK to filter messages when logging to an MCAP
 * file and/or a WebSocket server.
 *
 * Oftentimes, you may want to split "heavy" topics out into separate MCAP recordings, but still log
 * everything for live visualization. Splitting on topic in this way can be useful for selectively
 * retrieving data from bandwidth-constrained environments, such as with the Foxglove Agent.
 *
 * In this example, we log some point cloud data to one MCAP file, and some minimal metadata to
 * another.
 */
#include <foxglove/channel.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/server.hpp>

#include <atomic>
#include <chrono>
#include <cmath>
#include <csignal>
#include <functional>
#include <iostream>
#include <memory>
#include <thread>

using namespace std::chrono_literals;

using foxglove::schemas::PackedElementField;
using foxglove::schemas::PointCloud;
using foxglove::schemas::Pose;
using foxglove::schemas::Quaternion;
using foxglove::schemas::Vector3;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

/**
 * Generate an example point cloud.
 *
 * Adapted from https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors
 */
PointCloud make_point_cloud(const std::chrono::duration<double>& elapsed) {
  const double t = elapsed.count();
  std::vector<std::tuple<float, float, float, uint8_t, uint8_t, uint8_t, uint8_t>> points;

  for (int x = 0; x < 20; ++x) {
    for (int y = 0; y < 20; ++y) {
      const float x_coord =
        static_cast<float>(x) + static_cast<float>(std::cos(t + static_cast<float>(y) / 5.0f));
      const float y_coord = static_cast<float>(y);
      const float z_coord = 0.0f;

      const uint8_t r = static_cast<uint8_t>(255.0 * (0.5 + 0.5 * x_coord / 20.0f));
      const uint8_t g = static_cast<uint8_t>(255.0 * y_coord / 20.0f);
      const uint8_t b = static_cast<uint8_t>(255.0 * (0.5 + 0.5 * std::sin(t)));
      const uint8_t a =
        static_cast<uint8_t>(255.0 * (0.5 + 0.5 * ((x_coord / 20.0f) * (y_coord / 20.0f))));

      points.emplace_back(x_coord, y_coord, z_coord, r, g, b, a);
    }
  }

  // Pack data into bytes
  std::vector<std::byte> buffer;
  for (const auto& [x, y, z, r, g, b, a] : points) {
    const std::byte* x_bytes = reinterpret_cast<const std::byte*>(&x);
    const std::byte* y_bytes = reinterpret_cast<const std::byte*>(&y);
    const std::byte* z_bytes = reinterpret_cast<const std::byte*>(&z);

    buffer.insert(buffer.end(), x_bytes, x_bytes + sizeof(float));
    buffer.insert(buffer.end(), y_bytes, y_bytes + sizeof(float));
    buffer.insert(buffer.end(), z_bytes, z_bytes + sizeof(float));

    buffer.push_back(static_cast<std::byte>(r));
    buffer.push_back(static_cast<std::byte>(g));
    buffer.push_back(static_cast<std::byte>(b));
    buffer.push_back(static_cast<std::byte>(a));
  }

  // https://docs.foxglove.dev/docs/visualization/message-schemas/packed-element-field
  std::vector<PackedElementField> fields = {
    PackedElementField{
      "x",
      0,
      PackedElementField::NumericType::FLOAT32,
    },
    PackedElementField{
      "y",
      4,
      PackedElementField::NumericType::FLOAT32,
    },
    PackedElementField{
      "z",
      8,
      PackedElementField::NumericType::FLOAT32,
    },
    PackedElementField{
      "rgba",
      12,
      PackedElementField::NumericType::UINT32,
    },
  };

  PointCloud point_cloud;
  point_cloud.frame_id = "points";
  point_cloud.pose = Pose{
    Vector3{
      0.0,
      0.0,
      0.0,
    },
    Quaternion{
      0.0,
      0.0,
      0.0,
      1.0,
    },
  };
  point_cloud.point_stride = 16;  // 4 fields * 4 bytes
  point_cloud.fields = std::move(fields);
  point_cloud.data = std::move(buffer);

  return point_cloud;
}

/**
 * Create an MCAP writer with the specified channel filter.
 */
std::optional<foxglove::McapWriter> create_mcap_writer(
  const std::string& path, foxglove::SinkChannelFilterFn channel_filter
) {
  foxglove::McapWriterOptions options = {};
  options.path = path;
  options.truncate = true;
  options.sink_channel_filter = std::move(channel_filter);

  auto writer_result = foxglove::McapWriter::create(options);
  if (!writer_result.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_result.error()) << '\n';
    return std::nullopt;
  }
  return std::move(writer_result.value());
}

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  // Create channels for different message types
  auto info_channel_result = foxglove::RawChannel::create("/info", "json", std::nullopt);
  if (!info_channel_result.has_value()) {
    std::cerr << "Failed to create info channel: "
              << foxglove::strerror(info_channel_result.error()) << '\n';
    return 1;
  }
  auto info_channel = std::move(info_channel_result.value());

  auto point_cloud_channel_result = foxglove::schemas::PointCloudChannel::create("/point_cloud");
  if (!point_cloud_channel_result.has_value()) {
    std::cerr << "Failed to create point cloud channel: "
              << foxglove::strerror(point_cloud_channel_result.error()) << '\n';
    return 1;
  }
  auto point_cloud_channel = std::move(point_cloud_channel_result.value());

  auto point_cloud_tf_channel_result =
    foxglove::schemas::FrameTransformsChannel::create("/point_cloud_tf");
  if (!point_cloud_tf_channel_result.has_value()) {
    std::cerr << "Failed to create point cloud tf channel: "
              << foxglove::strerror(point_cloud_tf_channel_result.error()) << '\n';
    return 1;
  }
  auto point_cloud_tf_channel = std::move(point_cloud_tf_channel_result.value());

  // In one MCAP, drop all of our point_cloud (and related tf) messages
  auto small_writer = create_mcap_writer(
    "example-topic-splitting-small.mcap",
    [](foxglove::ChannelDescriptor&& channel) -> bool {
      return channel.topic().find("/point_cloud") == std::string::npos;
    }
  );
  if (!small_writer.has_value()) {
    return 1;
  }

  // In the other, log only the point_cloud (and related tf) messages
  auto large_writer = create_mcap_writer(
    "example-topic-splitting-large.mcap",
    [](foxglove::ChannelDescriptor&& channel) -> bool {
      return channel.topic().find("/point_cloud") != std::string::npos;
    }
  );
  if (!large_writer.has_value()) {
    return 1;
  }

  foxglove::WebSocketServerOptions ws_options = {};
  ws_options.name = "message-filtering-demo-cpp";
  ws_options.host = "127.0.0.1";
  ws_options.port = 8765;

  // We'll send all messages to the running app. We don't need a filter, since it's the same as
  // having no filter applied, but this demonstrates how to add one to the WS server.
  ws_options.sink_channel_filter = [](foxglove::ChannelDescriptor&&) -> bool {
    return true;
  };

  auto server_result = foxglove::WebSocketServer::create(std::move(ws_options));
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

  const auto start = std::chrono::system_clock::now();

  // Create a static transform for the point cloud
  foxglove::schemas::FrameTransform tf;
  tf.parent_frame_id = "world";
  tf.child_frame_id = "points";
  tf.translation = foxglove::schemas::Vector3{
    -10.0,
    -10.0,
    0.0,
  };
  foxglove::schemas::FrameTransforms point_cloud_tf{{tf}};

  while (!done) {
    const auto now = std::chrono::system_clock::now();
    const auto elapsed = now - start;

    // Generate state message
    const double t = std::cos(std::chrono::duration<double>(elapsed).count());
    const std::string state = (t > 0.0) ? "pos" : "neg";
    const std::string info_msg = "{\"state\": \"" + state + "\"}";
    const auto timestamp = std::chrono::nanoseconds(now.time_since_epoch()).count();
    info_channel.log(
      reinterpret_cast<const std::byte*>(info_msg.data()), info_msg.size(), timestamp
    );

    // Generate and log point cloud
    const auto point_cloud = make_point_cloud(std::chrono::duration<double>(elapsed));
    point_cloud_channel.log(point_cloud, timestamp);
    point_cloud_tf_channel.log(point_cloud_tf, timestamp);

    std::this_thread::sleep_for(33ms);
  }

  foxglove::FoxgloveError err = small_writer->close();
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to close writer: " << foxglove::strerror(err) << '\n';
    return 1;
  }
  err = large_writer->close();
  if (err != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to close writer: " << foxglove::strerror(err) << '\n';
    return 1;
  }

  std::cerr << "Done\n";
  return 0;
}
