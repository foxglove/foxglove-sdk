#include "mock_server.hpp"

#include <nlohmann/json.hpp>

#include <atomic>
#include <chrono>
#include <httplib.h>
#include <sstream>
#include <string>
#include <thread>

#include "livekit_token.hpp"

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
    : server_(std::make_unique<httplib::Server>())
    , stop_flag_(std::make_shared<std::atomic<bool>>(false)) {
  std::string room = room_name;
  auto stop_flag = stop_flag_;

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

  server_->Get(
    "/internal/platform/v1/remote-sessions/watch",
    [room, stop_flag](const httplib::Request& req, httplib::Response& res) {
      if (!validate_device_token(req)) {
        res.status = 401;
        return;
      }

      auto token = generate_token(room, TEST_DEVICE_ID);
      auto now_ns = std::chrono::duration_cast<std::chrono::nanoseconds>(
                      std::chrono::system_clock::now().time_since_epoch()
      )
                      .count();
      std::ostringstream lease_oss;
      lease_oss << "rwl_" << std::hex << now_ns;
      std::string lease_id = lease_oss.str();

      nlohmann::json hello = {
        {"watchLeaseId", lease_id},
        {"deviceWaitForViewerMs", 300000},
        {"heartbeatIntervalMs", 5000},
      };
      nlohmann::json wake = {
        {"remoteAccessSessionId", "ras_0000mockSession"},
        {"url", livekit_url()},
        {"token", token},
      };

      std::string hello_event = "event: hello\ndata: " + hello.dump() + "\n\n";
      std::string wake_event = "event: wake\ndata: " + wake.dump() + "\n\n";

      res.set_chunked_content_provider(
        "text/event-stream",
        [stop_flag, hello_event, wake_event](size_t offset, httplib::DataSink& sink) -> bool {
          if (offset == 0) {
            std::string events = hello_event + wake_event;
            if (!sink.write(events.data(), events.size())) {
              return false;
            }
            return true;
          }
          // Subsequent invocations: emit a keepalive comment every few seconds while the
          // server is alive. The gateway interprets any byte as proof of life.
          for (int i = 0; i < 50; ++i) {
            if (stop_flag->load()) {
              sink.done();
              return false;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
          }
          static const std::string keepalive = ": keepalive\n\n";
          if (!sink.write(keepalive.data(), keepalive.size())) {
            return false;
          }
          return true;
        }
      );
    }
  );

  server_->Post(
    "/internal/platform/v1/remote-sessions/watch/heartbeat",
    [](const httplib::Request& req, httplib::Response& res) {
      if (!validate_device_token(req)) {
        res.status = 401;
        return;
      }
      res.status = 200;
    }
  );

  int port = server_->bind_to_any_port("127.0.0.1");
  url_ = "http://127.0.0.1:" + std::to_string(port);

  thread_ = std::thread([this]() {
    server_->listen_after_bind();
  });
}

MockServerHandle::~MockServerHandle() {
  if (stop_flag_) {
    stop_flag_->store(true);
  }
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
    , url_(std::move(other.url_))
    , stop_flag_(std::move(other.stop_flag_)) {}

MockServerHandle& MockServerHandle::operator=(MockServerHandle&& other) noexcept {
  if (this != &other) {
    if (stop_flag_) {
      stop_flag_->store(true);
    }
    if (server_) {
      server_->stop();
    }
    if (thread_.joinable()) {
      thread_.join();
    }
    server_ = std::move(other.server_);
    thread_ = std::move(other.thread_);
    url_ = std::move(other.url_);
    stop_flag_ = std::move(other.stop_flag_);
  }
  return *this;
}

MockServerHandle start_mock_server(const std::string& room_name) {
  return MockServerHandle(room_name);
}

}  // namespace foxglove_integration
