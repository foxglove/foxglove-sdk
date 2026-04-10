#include "mock_server.hpp"

#include "livekit_token.hpp"

#include <httplib.h>
#include <nlohmann/json.hpp>

#include <string>

namespace foxglove_integration {

namespace {

constexpr const char* TEST_DEVICE_NAME = "test-device";
constexpr const char* TEST_PROJECT_ID = "prj_testproj";

bool validate_device_token(const httplib::Request& req) {
  auto it = req.headers.find("Authorization");
  if (it == req.headers.end()) {
    return false;
  }
  return it->second == std::string("DeviceToken ") + TEST_DEVICE_TOKEN;
}

}  // namespace

MockServerHandle::MockServerHandle(const std::string& room_name)
    : server_(std::make_unique<httplib::Server>()) {
  std::string room = room_name;

  server_->Get(
    "/internal/platform/v1/device-info",
    [](const httplib::Request& req, httplib::Response& res) {
      if (!validate_device_token(req)) {
        res.status = 401;
        return;
      }
      nlohmann::json body = {
        {"id", TEST_DEVICE_ID},
        {"name", TEST_DEVICE_NAME},
        {"projectId", TEST_PROJECT_ID},
        {"retainRecordingsSeconds", 3600},
      };
      res.set_content(body.dump(), "application/json");
    }
  );

  server_->Post(
    "/internal/platform/v1/devices/:device_id/remote-sessions",
    [room](const httplib::Request& req, httplib::Response& res) {
      if (!validate_device_token(req)) {
        res.status = 401;
        return;
      }
      auto device_id = req.path_params.at("device_id");
      if (device_id != TEST_DEVICE_ID) {
        res.status = 404;
        return;
      }
      auto token = generate_token(room, TEST_DEVICE_ID);
      nlohmann::json body = {
        {"token", token},
        {"url", livekit_url()},
        {"remoteAccessSessionId", "ras_0000mockSession"},
      };
      res.set_content(body.dump(), "application/json");
    }
  );

  int port = server_->bind_to_any_port("127.0.0.1");
  url_ = "http://127.0.0.1:" + std::to_string(port);

  thread_ = std::thread([this]() {
    server_->listen_after_bind();
  });
}

MockServerHandle::~MockServerHandle() {
  if (server_) {
    server_->stop();
  }
  if (thread_.joinable()) {
    thread_.join();
  }
}

MockServerHandle::MockServerHandle(MockServerHandle&& other) noexcept
    : server_(std::move(other.server_))
    , thread_(std::move(other.thread_))
    , url_(std::move(other.url_)) {}

MockServerHandle& MockServerHandle::operator=(MockServerHandle&& other) noexcept {
  if (this != &other) {
    if (server_) {
      server_->stop();
    }
    if (thread_.joinable()) {
      thread_.join();
    }
    server_ = std::move(other.server_);
    thread_ = std::move(other.thread_);
    url_ = std::move(other.url_);
  }
  return *this;
}

MockServerHandle start_mock_server(const std::string& room_name) {
  return MockServerHandle(room_name);
}

}  // namespace foxglove_integration
