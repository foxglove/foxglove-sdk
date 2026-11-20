#pragma once

#include <atomic>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <optional>

#include <diagnostic_updater/diagnostic_status_wrapper.hpp>

namespace foxglove_bridge {

constexpr char DIAGNOSTICS_HARDWARE_ID[] = "none";
constexpr char DIAGNOSTICS_TASK_WEBSOCKET_SERVER[] = "websocket_server";
constexpr char DIAGNOSTICS_TASK_REMOTE_ACCESS[] = "remote_access";
constexpr char DIAGNOSTICS_TASK_FORWARDING[] = "forwarding";

/// How long the forwarding task stays at WARN after a failure count last increased.
/// diagnostic_aggregator keeps only the latest status for each name and publishes on its own timer,
/// so a WARN that lasts a single report can be replaced before it is ever aggregated.
constexpr std::chrono::seconds DIAGNOSTICS_FORWARDING_WARN_HOLD{10};

/// Remote access gateway connection state. Mirrors foxglove::RemoteAccessConnectionStatus so that
/// diagnostics don't depend on the bridge being built with remote access support.
enum class RemoteAccessState : uint8_t {
  Connecting,
  Connected,
  ShuttingDown,
  Shutdown,
};

/// Cached health state behind the bridge's /diagnostics tasks.
///
/// The record and set methods may be called from any thread, including SDK callback threads, and
/// only update atomics. The report methods read that state from the diagnostic_updater timer, which
/// is the only place diagnostics are published.
class BridgeDiagnostics {
public:
  /// Records a remote access gateway status change. A transition from Connected back to Connecting
  /// counts as a reconnect.
  void setRemoteAccessState(RemoteAccessState state);

  /// Records a ROS subscription that could not be created.
  void recordFailedSubscription();

  /// Records incompatible-QoS events on an existing subscription. The subscription stays active and
  /// keeps receiving from compatible publishers.
  void recordIncompatibleQosEvents(uint64_t count);

  /// Records a client service call that failed before or while being sent to the ROS service.
  void recordFailedServiceCall();

  void reportRemoteAccess(diagnostic_updater::DiagnosticStatusWrapper& stat) const;

  /// Reports cumulative failure counts. The level is WARN, listing the failures since the warning
  /// began, until DIAGNOSTICS_FORWARDING_WARN_HOLD after a call that saw a count increase. After
  /// that it returns to OK, so a past failure doesn't keep the status at WARN until a restart.
  /// Must only be called from one thread.
  void reportForwarding(
    size_t activeSubscriptions, diagnostic_updater::DiagnosticStatusWrapper& stat,
    std::chrono::steady_clock::time_point now = std::chrono::steady_clock::now());

private:
  struct ForwardingCounts {
    uint64_t failedSubscriptions = 0;
    uint64_t incompatibleQosEvents = 0;
    uint64_t failedServiceCalls = 0;
  };

  std::atomic<RemoteAccessState> _remoteAccessState{RemoteAccessState::Connecting};
  std::atomic<uint64_t> _remoteAccessReconnects{0};
  std::atomic<uint64_t> _failedSubscriptions{0};
  std::atomic<uint64_t> _incompatibleQosEvents{0};
  std::atomic<uint64_t> _failedServiceCalls{0};
  ForwardingCounts _lastReportedForwarding;
  ForwardingCounts _forwardingWarnBaseline;
  std::optional<std::chrono::steady_clock::time_point> _lastForwardingIncrease;
};

void reportWebSocketServer(uint16_t port, size_t clientCount,
                           diagnostic_updater::DiagnosticStatusWrapper& stat);

}  // namespace foxglove_bridge
