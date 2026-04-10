#pragma once

#include <chrono>
#include <cstdint>
#include <functional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <thread>

#ifdef _WIN32
#include <process.h>
#else
#include <unistd.h>
#endif

namespace foxglove_integration {

constexpr auto EVENT_TIMEOUT = std::chrono::seconds(15);
constexpr auto READ_TIMEOUT = std::chrono::seconds(10);
constexpr auto SHUTDOWN_TIMEOUT = std::chrono::seconds(10);
constexpr auto POLL_INTERVAL = std::chrono::milliseconds(50);
constexpr auto CONNECT_RETRY_TIMEOUT = std::chrono::seconds(5);

inline void poll_until(
  const std::function<bool()>& cond,
  std::chrono::milliseconds timeout = std::chrono::duration_cast<std::chrono::milliseconds>(
    EVENT_TIMEOUT
  )
) {
  auto deadline = std::chrono::steady_clock::now() + timeout;
  while (!cond()) {
    if (std::chrono::steady_clock::now() >= deadline) {
      throw std::runtime_error("poll_until condition not met within timeout");
    }
    std::this_thread::sleep_for(POLL_INTERVAL);
  }
}

inline std::string unique_id() {
  auto now = std::chrono::system_clock::now().time_since_epoch();
  auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(now).count();
#ifdef _WIN32
  auto pid = _getpid();
#else
  auto pid = getpid();
#endif
  std::ostringstream ss;
  ss << std::hex << nanos << "-" << std::hex << pid;
  return ss.str();
}

}  // namespace foxglove_integration
