#pragma once

#include <atomic>
#include <memory>
#include <string>
#include <thread>

namespace httplib {
class Server;
}

namespace foxglove_integration {

constexpr const char* TEST_DEVICE_TOKEN = "fox_dt_testtoken";
constexpr const char* TEST_DEVICE_ID = "dev_testdevice";

/// RAII handle for a mock Foxglove API server.
/// The server runs on a background thread and is stopped on destruction.
class MockServerHandle {
public:
  MockServerHandle(const std::string& room_name);
  ~MockServerHandle();

  MockServerHandle(const MockServerHandle&) = delete;
  MockServerHandle& operator=(const MockServerHandle&) = delete;
  MockServerHandle(MockServerHandle&&) noexcept;
  MockServerHandle& operator=(MockServerHandle&&) noexcept;

  const std::string& url() const {
    return url_;
  }

private:
  std::unique_ptr<httplib::Server> server_;
  std::thread thread_;
  std::string url_;
  std::shared_ptr<std::atomic<bool>> stop_flag_;
};

/// Starts a mock Foxglove API server that returns LiveKit tokens for the local dev server.
MockServerHandle start_mock_server(const std::string& room_name);

}  // namespace foxglove_integration
