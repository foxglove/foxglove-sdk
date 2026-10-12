#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <string>

#include <diagnostic_updater/diagnostic_status_wrapper.hpp>

namespace foxglove_bridge {

constexpr char DIAGNOSTICS_HARDWARE_ID[] = "none";
constexpr char DIAGNOSTICS_TASK_BUILD_INFO[] = "build_info";
constexpr char DIAGNOSTICS_TASK_WEBSOCKET_SERVER[] = "websocket_server";
constexpr char DIAGNOSTICS_TASK_REMOTE_ACCESS[] = "remote_access";
constexpr char DIAGNOSTICS_TASK_FORWARDING[] = "forwarding";

/// Remote access gateway connection state. Mirrors foxglove::RemoteAccessConnectionStatus so that
/// diagnostics don't depend on the bridge being built with remote access support.
enum class RemoteAccessState : uint8_t {
  Connecting,
  Connected,
  ShuttingDown,
  Shutdown,
};

/// Static information reported by the build_info task.
struct BuildInfo {
  std::string bridgeVersion;
  std::string bridgeGitHash;
  std::string sdkVersion;
  std::string rosDistro;
  bool remoteAccessSupport = false;
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

  /// Records ROS subscriptions that could not be created, or incompatible-QoS events on existing
  /// subscriptions.
  void recordFailedSubscriptions(uint64_t count = 1);

  /// Records a client service call that failed before or while being sent to the ROS service.
  void recordFailedServiceCall();

  void reportRemoteAccess(diagnostic_updater::DiagnosticStatusWrapper& stat) const;
  void reportForwarding(size_t activeSubscriptions,
                        diagnostic_updater::DiagnosticStatusWrapper& stat) const;

private:
  std::atomic<RemoteAccessState> _remoteAccessState{RemoteAccessState::Connecting};
  std::atomic<uint64_t> _remoteAccessReconnects{0};
  std::atomic<uint64_t> _failedSubscriptions{0};
  std::atomic<uint64_t> _failedServiceCalls{0};
};

void reportBuildInfo(const BuildInfo& info, diagnostic_updater::DiagnosticStatusWrapper& stat);

void reportWebSocketServer(uint16_t port, size_t clientCount,
                           diagnostic_updater::DiagnosticStatusWrapper& stat);

}  // namespace foxglove_bridge
