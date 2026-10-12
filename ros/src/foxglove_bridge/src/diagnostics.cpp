#include "foxglove_bridge/diagnostics.hpp"

#include <diagnostic_msgs/msg/diagnostic_status.hpp>

namespace foxglove_bridge {

namespace {
using diagnostic_msgs::msg::DiagnosticStatus;

std::string pluralize(uint64_t count, const std::string& singular, const std::string& plural) {
  return std::to_string(count) + " " + (count == 1 ? singular : plural);
}
}  // namespace

void BridgeDiagnostics::setRemoteAccessState(RemoteAccessState state) {
  const auto previous = _remoteAccessState.exchange(state);
  if (previous == RemoteAccessState::Connected && state == RemoteAccessState::Connecting) {
    ++_remoteAccessReconnects;
  }
}

void BridgeDiagnostics::recordFailedSubscriptions(uint64_t count) {
  _failedSubscriptions += count;
}

void BridgeDiagnostics::recordFailedServiceCall() {
  ++_failedServiceCalls;
}

void BridgeDiagnostics::reportRemoteAccess(
  diagnostic_updater::DiagnosticStatusWrapper& stat) const {
  // Connected means the gateway's watch stream is up, whether or not a viewer is present.
  // ShuttingDown and Shutdown are only observed here if the gateway stopped on its own, e.g.
  // because the device token was rejected, since diagnostics stop before the bridge stops it.
  switch (_remoteAccessState.load()) {
    case RemoteAccessState::Connecting:
      stat.summary(DiagnosticStatus::WARN, "Connecting");
      break;
    case RemoteAccessState::Connected:
      stat.summary(DiagnosticStatus::OK, "Connected");
      break;
    case RemoteAccessState::ShuttingDown:
      stat.summary(DiagnosticStatus::ERROR, "Shutting down");
      break;
    case RemoteAccessState::Shutdown:
      stat.summary(DiagnosticStatus::ERROR, "Shut down");
      break;
  }
  stat.add("Reconnects", _remoteAccessReconnects.load());
}

void BridgeDiagnostics::reportForwarding(size_t activeSubscriptions,
                                         diagnostic_updater::DiagnosticStatusWrapper& stat) const {
  const uint64_t failedSubscriptions = _failedSubscriptions.load();
  const uint64_t failedServiceCalls = _failedServiceCalls.load();
  if (failedSubscriptions == 0 && failedServiceCalls == 0) {
    stat.summary(DiagnosticStatus::OK, "No failures");
  } else {
    stat.summary(DiagnosticStatus::WARN,
                 pluralize(failedSubscriptions, "failed subscription", "failed subscriptions") +
                   " and " +
                   pluralize(failedServiceCalls, "failed service call", "failed service calls") +
                   " since startup");
  }
  stat.add("Active subscriptions", activeSubscriptions);
  stat.add("Failed subscriptions", failedSubscriptions);
  stat.add("Failed service calls", failedServiceCalls);
}

void reportBuildInfo(const BuildInfo& info, diagnostic_updater::DiagnosticStatusWrapper& stat) {
  stat.summary(DiagnosticStatus::OK,
               "foxglove_bridge " + info.bridgeVersion + ", Foxglove SDK " + info.sdkVersion);
  stat.add("Bridge version", info.bridgeVersion);
  stat.add("Bridge git hash", info.bridgeGitHash);
  stat.add("SDK version", info.sdkVersion);
  stat.add("ROS distro", info.rosDistro);
  stat.add("Remote access support", info.remoteAccessSupport);
}

void reportWebSocketServer(uint16_t port, size_t clientCount,
                           diagnostic_updater::DiagnosticStatusWrapper& stat) {
  stat.summary(DiagnosticStatus::OK, pluralize(clientCount, "client", "clients") + " connected");
  stat.add("Port", port);
  stat.add("Clients", clientCount);
}

}  // namespace foxglove_bridge
