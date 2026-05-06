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

constexpr auto EVENT_TIMEOUT = std::chrono::seconds(8);
constexpr auto READ_TIMEOUT = std::chrono::seconds(5);
constexpr auto POLL_INTERVAL = std::chrono::milliseconds(50);

// Upper bound for waiting on a `DataTrackPublished` event from a remote
// participant. The gateway's `publish_data_track` task uses a 10s per-attempt
// timeout in the LiveKit Rust SDK and retries with exponential backoff (up to
// 3s) on transient errors. 15s covers one failed attempt plus a retry plus SFU
// announce flush, while leaving headroom within the 30s CTest per-test timeout
// even when ViewerConnection::connect uses part of its retry budget.
constexpr auto DATA_TRACK_PUBLISH_TIMEOUT = std::chrono::seconds(15);

inline void poll_until(
  const std::function<bool()>& cond,
  std::chrono::milliseconds timeout =
    std::chrono::duration_cast<std::chrono::milliseconds>(EVENT_TIMEOUT)
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
