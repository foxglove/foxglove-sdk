#include <optional>
#include <string>

#include <diagnostic_msgs/msg/diagnostic_status.hpp>
#include <diagnostic_updater/diagnostic_status_wrapper.hpp>
#include <gtest/gtest.h>

#include <foxglove_bridge/diagnostics.hpp>

using diagnostic_msgs::msg::DiagnosticStatus;
using diagnostic_updater::DiagnosticStatusWrapper;
using foxglove_bridge::BridgeDiagnostics;
using foxglove_bridge::RemoteAccessState;

namespace {
std::optional<std::string> valueOf(const DiagnosticStatus& stat, const std::string& key) {
  for (const auto& kv : stat.values) {
    if (kv.key == key) {
      return kv.value;
    }
  }
  return std::nullopt;
}

DiagnosticStatus remoteAccessStatus(const BridgeDiagnostics& diagnostics) {
  DiagnosticStatusWrapper stat;
  diagnostics.reportRemoteAccess(stat);
  return static_cast<const DiagnosticStatus&>(stat);
}

DiagnosticStatus forwardingStatus(const BridgeDiagnostics& diagnostics,
                                  size_t activeSubscriptions = 0) {
  DiagnosticStatusWrapper stat;
  diagnostics.reportForwarding(activeSubscriptions, stat);
  return static_cast<const DiagnosticStatus&>(stat);
}
}  // namespace

TEST(BridgeDiagnosticsTest, RemoteAccessStartsConnectingAsWarn) {
  BridgeDiagnostics diagnostics;
  const auto stat = remoteAccessStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::WARN);
  EXPECT_EQ(stat.message, "Connecting");
  EXPECT_EQ(valueOf(stat, "Reconnects"), "0");
}

TEST(BridgeDiagnosticsTest, RemoteAccessConnectedIsOk) {
  BridgeDiagnostics diagnostics;
  diagnostics.setRemoteAccessState(RemoteAccessState::Connecting);
  diagnostics.setRemoteAccessState(RemoteAccessState::Connected);
  const auto stat = remoteAccessStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::OK);
  EXPECT_EQ(stat.message, "Connected");
  EXPECT_EQ(valueOf(stat, "Reconnects"), "0");
}

TEST(BridgeDiagnosticsTest, RemoteAccessCountsLostConnectionsAsReconnects) {
  BridgeDiagnostics diagnostics;
  diagnostics.setRemoteAccessState(RemoteAccessState::Connecting);
  diagnostics.setRemoteAccessState(RemoteAccessState::Connected);
  diagnostics.setRemoteAccessState(RemoteAccessState::Connecting);

  auto stat = remoteAccessStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::WARN);
  EXPECT_EQ(valueOf(stat, "Reconnects"), "1");

  diagnostics.setRemoteAccessState(RemoteAccessState::Connected);
  diagnostics.setRemoteAccessState(RemoteAccessState::Connecting);
  diagnostics.setRemoteAccessState(RemoteAccessState::Connected);

  stat = remoteAccessStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::OK);
  EXPECT_EQ(valueOf(stat, "Reconnects"), "2");
}

TEST(BridgeDiagnosticsTest, RemoteAccessRetriesBeforeFirstConnectionAreNotReconnects) {
  BridgeDiagnostics diagnostics;
  diagnostics.setRemoteAccessState(RemoteAccessState::Connecting);
  diagnostics.setRemoteAccessState(RemoteAccessState::Connecting);
  EXPECT_EQ(valueOf(remoteAccessStatus(diagnostics), "Reconnects"), "0");
}

TEST(BridgeDiagnosticsTest, RemoteAccessShutdownIsErrorAndNotAReconnect) {
  BridgeDiagnostics diagnostics;
  diagnostics.setRemoteAccessState(RemoteAccessState::Connected);
  diagnostics.setRemoteAccessState(RemoteAccessState::ShuttingDown);

  auto stat = remoteAccessStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::ERROR);
  EXPECT_EQ(stat.message, "Shutting down");

  diagnostics.setRemoteAccessState(RemoteAccessState::Shutdown);
  stat = remoteAccessStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::ERROR);
  EXPECT_EQ(stat.message, "Shut down");
  EXPECT_EQ(valueOf(stat, "Reconnects"), "0");
}

TEST(BridgeDiagnosticsTest, ForwardingWithoutFailuresIsOk) {
  BridgeDiagnostics diagnostics;
  const auto stat = forwardingStatus(diagnostics, 3);
  EXPECT_EQ(stat.level, DiagnosticStatus::OK);
  EXPECT_EQ(stat.message, "No failures");
  EXPECT_EQ(valueOf(stat, "Active subscriptions"), "3");
  EXPECT_EQ(valueOf(stat, "Failed subscriptions"), "0");
  EXPECT_EQ(valueOf(stat, "Failed service calls"), "0");
}

TEST(BridgeDiagnosticsTest, ForwardingWarnsOnFailedSubscriptions) {
  BridgeDiagnostics diagnostics;
  diagnostics.recordFailedSubscriptions();
  diagnostics.recordFailedSubscriptions(2);
  const auto stat = forwardingStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::WARN);
  EXPECT_EQ(stat.message, "3 failed subscriptions and 0 failed service calls since startup");
  EXPECT_EQ(valueOf(stat, "Failed subscriptions"), "3");
  EXPECT_EQ(valueOf(stat, "Failed service calls"), "0");
}

TEST(BridgeDiagnosticsTest, ForwardingWarnsOnFailedServiceCalls) {
  BridgeDiagnostics diagnostics;
  diagnostics.recordFailedServiceCall();
  const auto stat = forwardingStatus(diagnostics);
  EXPECT_EQ(stat.level, DiagnosticStatus::WARN);
  EXPECT_EQ(stat.message, "0 failed subscriptions and 1 failed service call since startup");
  EXPECT_EQ(valueOf(stat, "Failed subscriptions"), "0");
  EXPECT_EQ(valueOf(stat, "Failed service calls"), "1");
}

TEST(BridgeDiagnosticsTest, WebSocketServerIsOkAndReportsClientCount) {
  DiagnosticStatusWrapper stat;
  foxglove_bridge::reportWebSocketServer(8765, 1, stat);
  EXPECT_EQ(stat.level, DiagnosticStatus::OK);
  EXPECT_EQ(stat.message, "1 client connected");
  EXPECT_EQ(valueOf(stat, "Port"), "8765");
  EXPECT_EQ(valueOf(stat, "Clients"), "1");
}
