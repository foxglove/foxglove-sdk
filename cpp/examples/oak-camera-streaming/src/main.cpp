#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/messages.hpp>
#include <foxglove/schema.hpp>
#include <foxglove/websocket.hpp>

#include <depthai/depthai.hpp>

#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <csignal>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <iostream>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <thread>
#include <utility>
#include <vector>

using namespace std::chrono_literals;

namespace {

constexpr float kNeuralFps = 30.0F;
constexpr float kStereoDefaultFps = 30.0F;
constexpr int kImuHz = 50;
constexpr std::pair<int, int> kColorSize = {640, 400};

constexpr std::string_view kCameraFrame = "oak";
constexpr std::string_view kOpticalFrame = "oak_optical";

constexpr uint32_t kPointStride = 16;
constexpr float kMillimetersToMeters = 0.001F;
constexpr float kAutoMillimeterZThreshold = 50.0F;

std::atomic_bool g_done = false;

enum class DepthSource {
  Stereo,
  Neural,
};

enum class PointUnit {
  Auto,
  Meters,
  Millimeters,
};

struct Args {
  uint16_t port = 8765;
  DepthSource depth_source = DepthSource::Stereo;
  PointUnit point_unit = PointUnit::Auto;
  std::string record_path;
};

void printUsage(const char* argv0) {
  std::cerr << "Usage: " << argv0 << " [--port PORT] [--depth-source stereo|neural]\n"
            << "       [--record PATH] [--point-unit auto|meters|millimeters]\n";
}

uint16_t parsePort(const std::string& value) {
  const int port = std::stoi(value);
  if (port <= 0 || port > 65535) {
    throw std::runtime_error("port must be between 1 and 65535");
  }
  return static_cast<uint16_t>(port);
}

Args parseArgs(int argc, char** argv) {
  Args args;
  for (int i = 1; i < argc; ++i) {
    const std::string arg = argv[i];
    auto require_value = [&](const std::string& name) -> std::string {
      if (i + 1 >= argc) {
        throw std::runtime_error(name + " requires a value");
      }
      return argv[++i];
    };

    if (arg == "--help" || arg == "-h") {
      printUsage(argv[0]);
      std::exit(0);
    }
    if (arg == "--port") {
      args.port = parsePort(require_value(arg));
    } else if (arg == "--depth-source") {
      const std::string value = require_value(arg);
      if (value == "stereo") {
        args.depth_source = DepthSource::Stereo;
      } else if (value == "neural") {
        args.depth_source = DepthSource::Neural;
      } else {
        throw std::runtime_error("--depth-source must be 'stereo' or 'neural'");
      }
    } else if (arg == "--record") {
      args.record_path = require_value(arg);
    } else if (arg == "--point-unit") {
      const std::string value = require_value(arg);
      if (value == "auto") {
        args.point_unit = PointUnit::Auto;
      } else if (value == "meters") {
        args.point_unit = PointUnit::Meters;
      } else if (value == "millimeters") {
        args.point_unit = PointUnit::Millimeters;
      } else {
        throw std::runtime_error("--point-unit must be 'auto', 'meters', or 'millimeters'");
      }
    } else {
      throw std::runtime_error("unknown argument: " + arg);
    }
  }
  return args;
}

std::string_view toString(DepthSource depth_source) {
  return depth_source == DepthSource::Neural ? "neural" : "stereo";
}

std::string_view toString(PointUnit point_unit) {
  switch (point_unit) {
    case PointUnit::Auto:
      return "auto";
    case PointUnit::Meters:
      return "meters";
    case PointUnit::Millimeters:
      return "millimeters";
  }
  return "auto";
}

uint64_t systemTimeNanos(std::chrono::system_clock::time_point time) {
  return static_cast<uint64_t>(
    std::chrono::duration_cast<std::chrono::nanoseconds>(time.time_since_epoch())
      .count()
  );
}

uint64_t nowNanos() {
  return systemTimeNanos(std::chrono::system_clock::now());
}

foxglove::messages::Timestamp timestampFromNanos(uint64_t nanos) {
  foxglove::messages::Timestamp timestamp;
  timestamp.sec = static_cast<uint32_t>(nanos / 1'000'000'000ULL);
  timestamp.nsec = static_cast<uint32_t>(nanos % 1'000'000'000ULL);
  return timestamp;
}

template<typename Duration>
foxglove::messages::Timestamp toTimestamp(
  const std::chrono::time_point<std::chrono::steady_clock, Duration>& time
) {
  static const auto steady_reference = std::chrono::steady_clock::now();
  static const auto system_reference = std::chrono::system_clock::now();

  const auto delta = time - steady_reference;
  const auto system_time =
    system_reference + std::chrono::duration_cast<std::chrono::system_clock::duration>(delta);
  const auto nanos =
    std::chrono::duration_cast<std::chrono::nanoseconds>(system_time.time_since_epoch());
  if (nanos.count() < 0) {
    return timestampFromNanos(nowNanos());
  }
  return timestampFromNanos(static_cast<uint64_t>(nanos.count()));
}

std::vector<foxglove::messages::PackedElementField> pointFields() {
  using Field = foxglove::messages::PackedElementField;
  using NumericType = foxglove::messages::PackedElementField::NumericType;
  return {
    Field{"x", 0, NumericType::FLOAT32},
    Field{"y", 4, NumericType::FLOAT32},
    Field{"z", 8, NumericType::FLOAT32},
    Field{"red", 12, NumericType::UINT8},
    Field{"green", 13, NumericType::UINT8},
    Field{"blue", 14, NumericType::UINT8},
    Field{"alpha", 15, NumericType::UINT8},
  };
}

void appendFloat32(std::vector<std::byte>& buffer, float value) {
  const auto* bytes = reinterpret_cast<const std::byte*>(&value);
  buffer.insert(buffer.end(), bytes, bytes + sizeof(value));
}

float pointScaleForUnit(const std::vector<dai::Point3fRGBA>& points, PointUnit point_unit) {
  if (point_unit == PointUnit::Meters) {
    return 1.0F;
  }
  if (point_unit == PointUnit::Millimeters) {
    return kMillimetersToMeters;
  }

  std::vector<float> z_values;
  z_values.reserve(points.size());
  for (const auto& point : points) {
    if (point.z > 0.0F) {
      z_values.push_back(point.z);
    }
  }
  if (z_values.empty()) {
    return 1.0F;
  }

  const auto median = z_values.begin() + static_cast<std::ptrdiff_t>(z_values.size() / 2);
  std::nth_element(z_values.begin(), median, z_values.end());
  return *median > kAutoMillimeterZThreshold ? kMillimetersToMeters : 1.0F;
}

foxglove::messages::PointCloud pointCloudToMessage(
  dai::PointCloudData& pcl, PointUnit point_unit
) {
  auto points = pcl.getPointsRGB();
  const float scale = pointScaleForUnit(points, point_unit);

  std::vector<std::byte> buffer;
  buffer.reserve(points.size() * kPointStride);

  for (const auto& point : points) {
    if (point.z <= 0.0F) {
      continue;
    }

    appendFloat32(buffer, point.x * scale);
    appendFloat32(buffer, point.y * scale);
    appendFloat32(buffer, point.z * scale);
    buffer.push_back(static_cast<std::byte>(point.r));
    buffer.push_back(static_cast<std::byte>(point.g));
    buffer.push_back(static_cast<std::byte>(point.b));
    buffer.push_back(static_cast<std::byte>(255));
  }

  foxglove::messages::PointCloud msg;
  msg.timestamp = toTimestamp(pcl.getTimestamp());
  msg.frame_id = std::string(kOpticalFrame);
  msg.point_stride = kPointStride;
  msg.fields = pointFields();
  msg.data = std::move(buffer);
  return msg;
}

foxglove::messages::RawImage rawImageToMessage(dai::ImgFrame& frame) {
  const auto data = frame.getData();

  foxglove::messages::RawImage msg;
  msg.timestamp = toTimestamp(frame.getTimestamp());
  msg.frame_id = std::string(kOpticalFrame);
  msg.width = frame.getWidth();
  msg.height = frame.getHeight();
  msg.encoding = "bgr8";
  msg.step = msg.width * 3;
  msg.data.assign(
    reinterpret_cast<const std::byte*>(data.data()),
    reinterpret_cast<const std::byte*>(data.data() + data.size())
  );
  return msg;
}

std::array<double, 9> flattenIntrinsics(const std::vector<std::vector<float>>& intrinsics) {
  if (intrinsics.size() != 3 || intrinsics[0].size() != 3 || intrinsics[1].size() != 3 ||
      intrinsics[2].size() != 3) {
    throw std::runtime_error("DepthAI returned an invalid camera intrinsic matrix");
  }

  return {
    intrinsics[0][0],
    intrinsics[0][1],
    intrinsics[0][2],
    intrinsics[1][0],
    intrinsics[1][1],
    intrinsics[1][2],
    intrinsics[2][0],
    intrinsics[2][1],
    intrinsics[2][2],
  };
}

foxglove::messages::CameraCalibration buildCameraCalibration(
  const dai::CalibrationHandler& calibration, dai::CameraBoardSocket socket, uint32_t width,
  uint32_t height
) {
  foxglove::messages::CameraCalibration msg;
  msg.frame_id = std::string(kOpticalFrame);
  msg.width = width;
  msg.height = height;
  msg.k = flattenIntrinsics(calibration.getCameraIntrinsics(socket, width, height));

  const double fx = msg.k[0];
  const double cx = msg.k[2];
  const double fy = msg.k[4];
  const double cy = msg.k[5];
  msg.r = {1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0};
  msg.p = {fx, 0.0, cx, 0.0, 0.0, fy, cy, 0.0, 0.0, 0.0, 1.0, 0.0};

  const auto distortion_coefficients = calibration.getDistortionCoefficients(socket);
  const auto distortion_model = calibration.getDistortionModel(socket);
  if (distortion_model == dai::CameraModel::Perspective) {
    msg.distortion_model = "rational_polynomial";
    const auto coeff_count = std::min<size_t>(8, distortion_coefficients.size());
    msg.d.assign(distortion_coefficients.begin(), distortion_coefficients.begin() + coeff_count);
  } else if (distortion_model == dai::CameraModel::Fisheye) {
    msg.distortion_model = "kannala_brandt";
    const auto coeff_count = std::min<size_t>(4, distortion_coefficients.size());
    msg.d.assign(distortion_coefficients.begin(), distortion_coefficients.begin() + coeff_count);
  } else {
    std::cerr << "Unsupported DepthAI distortion model; publishing intrinsics only\n";
  }

  return msg;
}

foxglove::messages::FrameTransforms makeStaticTransform() {
  foxglove::messages::FrameTransform transform;
  transform.timestamp = timestampFromNanos(nowNanos());
  transform.parent_frame_id = std::string(kCameraFrame);
  transform.child_frame_id = std::string(kOpticalFrame);
  transform.translation = foxglove::messages::Vector3{0.0, 0.0, 0.0};
  transform.rotation = foxglove::messages::Quaternion{-0.5, 0.5, -0.5, 0.5};
  return foxglove::messages::FrameTransforms{{std::move(transform)}};
}

std::string imuJson(const dai::IMUPacket& packet) {
  const auto stamp = toTimestamp(packet.acceleroMeter.getTimestamp());
  const auto& accel = packet.acceleroMeter;
  const auto& gyro = packet.gyroscope;

  return "{\"header\":{\"stamp\":{\"sec\":" + std::to_string(stamp.sec) +
         ",\"nsec\":" + std::to_string(stamp.nsec) + "},\"frame_id\":\"" +
         std::string(kOpticalFrame) + "\"},\"angular_velocity\":{\"x\":" +
         std::to_string(gyro.x) + ",\"y\":" + std::to_string(gyro.y) +
         ",\"z\":" + std::to_string(gyro.z) + "},\"linear_acceleration\":{\"x\":" +
         std::to_string(accel.x) + ",\"y\":" + std::to_string(accel.y) +
         ",\"z\":" + std::to_string(accel.z) + "}}";
}

foxglove::RawChannel createImuChannel() {
  foxglove::Schema schema;
  schema.name = "sensor_msgs.msg.ImuLike";
  schema.encoding = "jsonschema";
  const std::string schema_data = R"({
    "type": "object",
    "properties": {
      "header": {
        "type": "object",
        "properties": {
          "stamp": {
            "type": "object",
            "properties": {
              "sec": { "type": "integer" },
              "nsec": { "type": "integer" }
            }
          },
          "frame_id": { "type": "string" }
        }
      },
      "angular_velocity": {
        "type": "object",
        "properties": {
          "x": { "type": "number" },
          "y": { "type": "number" },
          "z": { "type": "number" }
        }
      },
      "linear_acceleration": {
        "type": "object",
        "properties": {
          "x": { "type": "number" },
          "y": { "type": "number" },
          "z": { "type": "number" }
        }
      }
    }
  })";
  schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
  schema.data_len = schema_data.size();

  auto result = foxglove::RawChannel::create("/oak/imu", "json", std::move(schema));
  if (!result.has_value()) {
    throw std::runtime_error(
      "Failed to create IMU channel: " + std::string(foxglove::strerror(result.error()))
    );
  }
  return std::move(result.value());
}

std::optional<foxglove::McapWriter> createMcapWriter(const std::string& path) {
  if (path.empty()) {
    return std::nullopt;
  }

  foxglove::McapWriterOptions options;
  options.path = path;
  auto result = foxglove::McapWriter::create(options);
  if (!result.has_value()) {
    throw std::runtime_error(
      "Failed to create MCAP writer: " + std::string(foxglove::strerror(result.error()))
    );
  }
  return std::move(result.value());
}

struct PipelineOutputs {
  std::shared_ptr<dai::node::RGBD> rgbd;
  dai::Node::Output* color_out = nullptr;
  dai::Node::Output* imu_out = nullptr;
  dai::CameraBoardSocket color_socket = dai::CameraBoardSocket::CAM_A;
};

PipelineOutputs buildPipeline(dai::Pipeline& pipeline, DepthSource depth_source) {
  const float fps = depth_source == DepthSource::Neural ? kNeuralFps : kStereoDefaultFps;

  auto color = pipeline.create<dai::node::Camera>();
  color->build(dai::CameraBoardSocket::CAM_A, std::nullopt, fps);
  auto* color_out = color->requestOutput(
    kColorSize, dai::ImgFrame::Type::BGR888i, dai::ImgResizeMode::CROP, std::nullopt, true
  );
  if (color_out == nullptr) {
    throw std::runtime_error("Failed to create color camera output");
  }

  dai::node::DepthSource depth_node;
  if (depth_source == DepthSource::Stereo) {
    auto left = pipeline.create<dai::node::Camera>();
    auto right = pipeline.create<dai::node::Camera>();
    auto stereo = pipeline.create<dai::node::StereoDepth>();

    left->build(dai::CameraBoardSocket::CAM_B, std::nullopt, fps);
    right->build(dai::CameraBoardSocket::CAM_C, std::nullopt, fps);

    stereo->setDefaultProfilePreset(dai::node::StereoDepth::PresetMode::DEFAULT);
    stereo->setRectifyEdgeFillColor(0);
    stereo->enableDistortionCorrection(true);
    left->requestOutput(kColorSize, std::nullopt, dai::ImgResizeMode::CROP)->link(stereo->left);
    right->requestOutput(kColorSize, std::nullopt, dai::ImgResizeMode::CROP)->link(stereo->right);
    depth_node = stereo;
  } else {
    auto left = pipeline.create<dai::node::Camera>();
    auto right = pipeline.create<dai::node::Camera>();
    auto neural_depth = pipeline.create<dai::node::NeuralDepth>();

    left->build(dai::CameraBoardSocket::CAM_B, std::nullopt, fps);
    right->build(dai::CameraBoardSocket::CAM_C, std::nullopt, fps);
    neural_depth->build(
      *left->requestFullResolutionOutput(), *right->requestFullResolutionOutput(),
      dai::DeviceModelZoo::NEURAL_DEPTH_LARGE
    );
    depth_node = neural_depth;
  }

  auto rgbd = pipeline.create<dai::node::RGBD>();
  rgbd->build(color, depth_node, kColorSize, fps);
  rgbd->setDepthUnit(dai::DepthUnit::METER);

  auto imu = pipeline.create<dai::node::IMU>();
  imu->enableIMUSensor(dai::IMUSensor::ACCELEROMETER_UNCALIBRATED, kImuHz);
  imu->enableIMUSensor(dai::IMUSensor::GYROSCOPE_UNCALIBRATED, kImuHz);
  imu->setBatchReportThreshold(5);
  imu->setMaxBatchReports(20);

  return PipelineOutputs{std::move(rgbd), color_out, &imu->out, dai::CameraBoardSocket::CAM_A};
}

}  // namespace

int main(int argc, char** argv) {
  std::signal(SIGINT, [](int) {
    g_done = true;
  });

  try {
    const Args args = parseArgs(argc, argv);
    foxglove::setLogLevel(foxglove::LogLevel::Info);

    auto writer = createMcapWriter(args.record_path);

    foxglove::WebSocketServerOptions ws_options;
    ws_options.name = "oak-camera-streaming-cpp";
    ws_options.host = "127.0.0.1";
    ws_options.port = args.port;
    auto server_result = foxglove::WebSocketServer::create(std::move(ws_options));
    if (!server_result.has_value()) {
      std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error())
                << '\n';
      return 1;
    }
    auto server = std::move(server_result.value());

    auto points_result = foxglove::messages::PointCloudChannel::create("/oak/points");
    auto rgb_result = foxglove::messages::RawImageChannel::create("/oak/rgb/image");
    auto cal_result =
      foxglove::messages::CameraCalibrationChannel::create("/oak/rgb/calibration");
    auto tf_result = foxglove::messages::FrameTransformsChannel::create("/tf");
    if (!points_result.has_value() || !rgb_result.has_value() || !cal_result.has_value() ||
        !tf_result.has_value()) {
      std::cerr << "Failed to create one or more Foxglove channels\n";
      return 1;
    }
    auto points_channel = std::move(points_result.value());
    auto rgb_channel = std::move(rgb_result.value());
    auto cal_channel = std::move(cal_result.value());
    auto imu_channel = createImuChannel();
    auto tf_channel = std::move(tf_result.value());

    dai::Pipeline pipeline;
    auto outputs = buildPipeline(pipeline, args.depth_source);

    auto pcl_queue = outputs.rgbd->pcl.createOutputQueue(4, false);
    auto rgb_queue = outputs.color_out->createOutputQueue(2, false);
    auto imu_queue = outputs.imu_out->createOutputQueue(50, false);

    pipeline.start();
    std::cerr << "Foxglove server listening at ws://127.0.0.1:" << server.port() << '\n';
    std::cerr << "Pipeline running with depth source '" << toString(args.depth_source)
              << "', point unit '" << toString(args.point_unit) << "', IMU " << kImuHz
              << "Hz. Press Ctrl+C to stop.\n";

    const auto device = pipeline.getDefaultDevice();
    auto calibration = buildCameraCalibration(
      device->readCalibration(), outputs.color_socket, kColorSize.first, kColorSize.second
    );

    while (!g_done && pipeline.isRunning()) {
      tf_channel.log(makeStaticTransform());

      if (auto pcl = pcl_queue->tryGet<dai::PointCloudData>(); pcl != nullptr && pcl->isColor()) {
        points_channel.log(pointCloudToMessage(*pcl, args.point_unit));
      }

      if (auto frame = rgb_queue->tryGet<dai::ImgFrame>(); frame != nullptr) {
        auto image = rawImageToMessage(*frame);
        const auto timestamp = image.timestamp;
        rgb_channel.log(image);

        calibration.timestamp = timestamp;
        cal_channel.log(calibration);
      }

      if (auto imu_data = imu_queue->tryGet<dai::IMUData>(); imu_data != nullptr) {
        for (const auto& packet : imu_data->packets) {
          const auto payload = imuJson(packet);
          imu_channel.log(
            reinterpret_cast<const std::byte*>(payload.data()), payload.size()
          );
        }
      }

      std::this_thread::sleep_for(1ms);
    }

    pipeline.stop();
    server.stop();
    if (writer.has_value()) {
      writer->close();
      std::cerr << "MCAP written to " << args.record_path << '\n';
    }
  } catch (const std::exception& err) {
    std::cerr << err.what() << '\n';
    printUsage(argv[0]);
    return 1;
  }

  return 0;
}
