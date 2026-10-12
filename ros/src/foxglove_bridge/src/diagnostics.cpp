#include "foxglove_bridge/diagnostics.hpp"

#include <string>

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

void BridgeDiagnostics::recordFailedSubscription() {
  ++_failedSubscriptions;
}

void BridgeDiagnostics::recordIncompatibleQosEvents(uint64_t count) {
  _incompatibleQosEvents += count;
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
                                         diagnostic_updater::DiagnosticStatusWrapper& stat) {
  const ForwardingCounts counts{_failedSubscriptions.load(), _incompatibleQosEvents.load(),
                                _failedServiceCalls.load()};
  const ForwardingCounts last = _lastReportedForwarding;

  std::string newFailures;
  const auto addNewFailures = [&newFailures](uint64_t current, uint64_t previous,
                                             const std::string& singular,
                                             const std::string& plural) {
    if (current > previous) {
      newFailures +=
        (newFailures.empty() ? "" : ", ") + pluralize(current - previous, singular, plural);
    }
  };
  addNewFailures(counts.failedSubscriptions, last.failedSubscriptions, "failed subscription",
                 "failed subscriptions");
  addNewFailures(counts.incompatibleQosEvents, last.incompatibleQosEvents, "incompatible QoS event",
                 "incompatible QoS events");
  addNewFailures(counts.failedServiceCalls, last.failedServiceCalls, "failed service call",
                 "failed service calls");
  _lastReportedForwarding = counts;

  if (newFailures.empty()) {
    stat.summary(DiagnosticStatus::OK, "No new failures");
  } else {
    stat.summary(DiagnosticStatus::WARN, newFailures + " since the last report");
  }
  stat.add("Active subscriptions", activeSubscriptions);
  stat.add("Failed subscriptions", counts.failedSubscriptions);
  stat.add("Incompatible QoS events", counts.incompatibleQosEvents);
  stat.add("Failed service calls", counts.failedServiceCalls);
}

void reportWebSocketServer(uint16_t port, size_t clientCount,
                           diagnostic_updater::DiagnosticStatusWrapper& stat) {
  stat.summary(DiagnosticStatus::OK, pluralize(clientCount, "client", "clients") + " connected");
  stat.add("Port", port);
  stat.add("Clients", clientCount);
}

}  // namespace foxglove_bridge
