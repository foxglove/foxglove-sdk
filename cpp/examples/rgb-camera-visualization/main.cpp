#include <foxglove/foxglove.hpp>
#include <foxglove/schemas.hpp>
#include <foxglove/server.hpp>

#include <opencv2/opencv.hpp>

#include <chrono>
#include <cstring>
#include <iostream>
#include <memory>
#include <string>

// Helper to parse command line arguments
std::string parse_camera_id(int argc, char** argv) {
  std::string camera_id = "0";
  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if ((arg == "--camera-id" || arg == "-c") && i + 1 < argc) {
      camera_id = argv[i + 1];
      ++i;
    }
  }
  return camera_id;
}

class CameraCapture {
public:
  CameraCapture(const std::string& camera_id)
      : camera_id_(camera_id) {}
  bool connect() {
    try {
      int cam_id = 0;
      try {
        cam_id = std::stoi(camera_id_);
        cap_.open(cam_id);
      } catch (...) {
        cap_.open(camera_id_);
      }
      if (!cap_.isOpened()) {
        std::cerr << "Failed to open camera " << camera_id_ << std::endl;
        return false;
      }
      int width = static_cast<int>(cap_.get(cv::CAP_PROP_FRAME_WIDTH));
      int height = static_cast<int>(cap_.get(cv::CAP_PROP_FRAME_HEIGHT));
      double fps = cap_.get(cv::CAP_PROP_FPS);
      std::cout << "Camera connected successfully:\n";
      std::cout << "  ID/Path: " << camera_id_ << "\n";
      std::cout << "  Resolution: " << width << " x " << height << "\n";
      std::cout << "  Frame Rate: " << fps << " fps\n";
      return true;
    } catch (const std::exception& e) {
      std::cerr << "Error connecting to camera: " << e.what() << std::endl;
      return false;
    }
  }
  bool read_frame(cv::Mat& frame) {
    if (!cap_.isOpened()) return false;
    return cap_.read(frame) && !frame.empty();
  }
  void disconnect() {
    if (cap_.isOpened()) cap_.release();
  }
  std::string camera_id_;
  cv::VideoCapture cap_;
};

foxglove::schemas::RawImage create_raw_image_message(const cv::Mat& frame) {
  int height = frame.rows;
  int width = frame.cols;
  int channels = frame.channels();

  // Convert uint8_t data to std::byte
  size_t data_size = frame.total() * frame.elemSize();
  std::vector<std::byte> data(data_size);
  std::memcpy(data.data(), frame.data, data_size);

  foxglove::schemas::RawImage msg;
  msg.width = width;
  msg.height = height;
  msg.step = width * channels;
  msg.encoding = "bgr8";
  msg.frame_id = "camera";
  msg.data = std::move(data);

  // Create timestamp manually
  auto now = std::chrono::system_clock::now();
  auto time_since_epoch = now.time_since_epoch();
  auto seconds = std::chrono::duration_cast<std::chrono::seconds>(time_since_epoch);
  auto nanoseconds =
    std::chrono::duration_cast<std::chrono::nanoseconds>(time_since_epoch - seconds);

  foxglove::schemas::Timestamp timestamp;
  timestamp.sec = static_cast<uint32_t>(seconds.count());
  timestamp.nsec = static_cast<uint32_t>(nanoseconds.count());
  msg.timestamp = timestamp;

  return msg;
}

int main(int argc, char** argv) {
  std::string camera_id = parse_camera_id(argc, argv);
  CameraCapture camera(camera_id);
  if (!camera.connect()) {
    std::cerr << "Failed to connect to camera. Exiting." << std::endl;
    return 1;
  }

  foxglove::WebSocketServerOptions ws_options;
  ws_options.host = "127.0.0.1";
  ws_options.port = 8765;
  auto server_result = foxglove::WebSocketServer::create(std::move(ws_options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error())
              << std::endl;
    return 1;
  }
  auto server = std::move(server_result.value());
  std::cout << "Foxglove server started on port " << server.port() << std::endl;

  auto image_channel_result = foxglove::schemas::RawImageChannel::create("/camera/image");
  if (!image_channel_result.has_value()) {
    std::cerr << "Failed to create image channel: "
              << foxglove::strerror(image_channel_result.error()) << std::endl;
    return 1;
  }
  auto image_channel = std::move(image_channel_result.value());

  std::cout << "Starting camera feed... Press Ctrl+C to stop." << std::endl;
  try {
    while (true) {
      cv::Mat frame;
      if (!camera.read_frame(frame)) {
        std::cerr << "Failed to read frame from camera" << std::endl;
        continue;
      }
      auto img_msg = create_raw_image_message(frame);
      image_channel.log(img_msg);
    }
  } catch (...) {
    std::cout << "\nShutting down camera visualization..." << std::endl;
  }
  camera.disconnect();
  std::cout << "Camera visualization stopped." << std::endl;
  return 0;
}
