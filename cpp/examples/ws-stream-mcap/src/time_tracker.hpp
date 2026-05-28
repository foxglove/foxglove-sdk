#pragma once

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <optional>

/// Tracks the relationship between file timestamps and wall-clock time.
///
/// Converts between "log time" (nanosecond timestamps in the MCAP file) and real wall-clock
/// time, accounting for playback speed, pause/resume, and speed changes.
class TimeTracker {
public:
  static constexpr float kMinPlaybackSpeed = 0.01f;

  TimeTracker(uint64_t offset_ns, float speed)
      : start_(std::chrono::steady_clock::now())
      , offset_ns_(offset_ns)
      , speed_(clampSpeed(speed))
      , paused_(false)
      , paused_elapsed_ns_(0)
      , notify_interval_ns_(1'000'000'000 / 60)
      , notify_last_(0) {}

  /// Returns the current log time based on elapsed wall time and playback speed.
  uint64_t currentLogTime() const {
    if (paused_) {
      return offset_ns_ + paused_elapsed_ns_;
    }
    auto elapsed_wall = std::chrono::steady_clock::now() - start_;
    auto elapsed_nanos = static_cast<uint64_t>(
      std::chrono::duration<double, std::nano>(elapsed_wall).count() * static_cast<double>(speed_)
    );
    return offset_ns_ + paused_elapsed_ns_ + elapsed_nanos;
  }

  /// Returns the wall-clock time point at which a message with the given log_time should be
  /// emitted.
  std::chrono::steady_clock::time_point wakeupFor(uint64_t log_time) const {
    uint64_t current = currentLogTime();
    if (log_time <= current) {
      return std::chrono::steady_clock::now();
    }
    uint64_t log_diff_ns = log_time - current;
    uint64_t wall_diff_ns;
    if (speed_ > 0.0f) {
      wall_diff_ns =
        static_cast<uint64_t>(static_cast<double>(log_diff_ns) / static_cast<double>(speed_));
    } else {
      wall_diff_ns = 1'000'000'000;
    }
    return std::chrono::steady_clock::now() + std::chrono::nanoseconds(wall_diff_ns);
  }

  /// Pauses time tracking, accumulating elapsed log time.
  void pause() {
    if (!paused_) {
      auto elapsed_wall = std::chrono::steady_clock::now() - start_;
      auto elapsed_nanos = static_cast<uint64_t>(
        std::chrono::duration<double, std::nano>(elapsed_wall).count() * static_cast<double>(speed_)
      );
      paused_elapsed_ns_ += elapsed_nanos;
      paused_ = true;
    }
  }

  /// Resumes time tracking from where it was paused.
  void resume() {
    if (paused_) {
      start_ = std::chrono::steady_clock::now();
      paused_ = false;
    }
  }

  /// Changes the playback speed, accumulating elapsed time at the old speed.
  void setSpeed(float speed) {
    speed = clampSpeed(speed);
    if (!paused_) {
      auto elapsed_wall = std::chrono::steady_clock::now() - start_;
      auto elapsed_nanos = static_cast<uint64_t>(
        std::chrono::duration<double, std::nano>(elapsed_wall).count() * static_cast<double>(speed_)
      );
      paused_elapsed_ns_ += elapsed_nanos;
      start_ = std::chrono::steady_clock::now();
    }
    speed_ = speed;
  }

  /// Clamps speed to a minimum value.
  static float clampSpeed(float speed) {
    if (std::isfinite(speed) && speed >= kMinPlaybackSpeed) {
      return speed;
    }
    return kMinPlaybackSpeed;
  }

  /// Returns the current log time if enough time has passed since the last notification (~60 Hz).
  std::optional<uint64_t> notify(uint64_t current_ns) {
    if (current_ns - notify_last_ >= notify_interval_ns_) {
      notify_last_ = current_ns;
      return current_ns;
    }
    return std::nullopt;
  }

private:
  std::chrono::steady_clock::time_point start_;
  uint64_t offset_ns_;
  float speed_;
  bool paused_;
  uint64_t paused_elapsed_ns_;
  uint64_t notify_interval_ns_;
  uint64_t notify_last_;
};
