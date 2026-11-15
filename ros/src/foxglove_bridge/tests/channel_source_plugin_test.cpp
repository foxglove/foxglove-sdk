#include <algorithm>
#include <chrono>
#include <cstdarg>
#include <cstdio>
#include <future>
#include <mutex>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include <gtest/gtest.h>
#include <rclcpp/rclcpp.hpp>
#include <rcutils/logging.h>
// See smoke_test.cpp for why the no-TLS client config is used.
#include <websocketpp/config/asio_no_tls_client.hpp>

#include <foxglove_bridge/param_utils.hpp>
#include <foxglove_bridge/ros2_foxglove_bridge.hpp>

#include "client/test_client.hpp"

namespace {

constexpr char TEST_PLUGIN_TOPIC[] = "/test_plugin/data";
constexpr char TEST_PLUGIN_MESSAGE[] = R"({"value":42})";
constexpr char TEST_PLUGIN_CLASS[] = "foxglove_bridge_test_plugins::TestChannelSourcePlugin";
constexpr char SECOND_INSTANCE_TOPIC[] = "/second_instance/data";
constexpr auto DEFAULT_TIMEOUT = std::chrono::seconds(10);
// Name of the bridge node that the tests connect to, and the prefix of its loggers.
constexpr char BRIDGE_NODE_NAME[] = "foxglove_bridge";
// smoke_test uses 8765 and 8766.
constexpr int PORT = 8767;
constexpr char URI[] = "ws://localhost:8767";

// Set in main once the bridge has started.
std::vector<std::string> g_startupErrors;

std::mutex g_logsMutex;
std::vector<std::string> g_errorLogs;
std::vector<std::string> g_infoLogs;
rcutils_logging_output_handler_t g_defaultLogHandler = nullptr;

bool isBridgeLogger(const char* name) {
  return name != nullptr && std::string(name).rfind(BRIDGE_NODE_NAME, 0) == 0;
}

void captureLogs(const rcutils_log_location_t* location, int severity, const char* name,
                 rcutils_time_point_value_t timestamp, const char* format, va_list* args) {
  if (severity >= RCUTILS_LOG_SEVERITY_INFO) {
    va_list argsCopy;
    va_copy(argsCopy, *args);
    char buffer[1024];
    std::vsnprintf(buffer, sizeof(buffer), format, argsCopy);
    va_end(argsCopy);
    std::lock_guard<std::mutex> lock(g_logsMutex);
    if (severity < RCUTILS_LOG_SEVERITY_ERROR) {
      g_infoLogs.emplace_back(buffer);
    } else if (isBridgeLogger(name)) {
      g_errorLogs.emplace_back(buffer);
    }
  }
  g_defaultLogHandler(location, severity, name, timestamp, format, args);
}

/// Returns and clears the errors logged by the bridge under test.
std::vector<std::string> takeErrorLogs() {
  std::lock_guard<std::mutex> lock(g_logsMutex);
  return std::exchange(g_errorLogs, {});
}

void clearInfoLogs() {
  std::lock_guard<std::mutex> lock(g_logsMutex);
  g_infoLogs.clear();
}

bool contains(const std::vector<std::string>& logs, const std::string& text) {
  return std::any_of(logs.begin(), logs.end(), [&text](const std::string& log) {
    return log.find(text) != std::string::npos;
  });
}

bool infoLogged(const std::string& text) {
  std::lock_guard<std::mutex> lock(g_logsMutex);
  return contains(g_infoLogs, text);
}

/// Options for a short-lived bridge that loads an instance of the test plugin named `probe`.
rclcpp::NodeOptions probeBridgeOptions(const std::string& nodeName) {
  rclcpp::NodeOptions nodeOptions;
  nodeOptions.arguments({"--ros-args", "-r", "__node:=" + nodeName});
  nodeOptions.append_parameter_override(foxglove_bridge::PARAM_PORT, 0);
  nodeOptions.append_parameter_override(foxglove_bridge::PARAM_SYSINFO, false);
  nodeOptions.append_parameter_override(foxglove_bridge::PARAM_CHANNEL_SOURCE_PLUGINS,
                                        std::vector<std::string>({"probe"}));
  nodeOptions.append_parameter_override("probe.plugin", TEST_PLUGIN_CLASS);
  return nodeOptions;
}

}  // namespace

TEST(ChannelSourcePluginTest, pluginChannelIsAdvertisedAndDeliversMessages) {
  foxglove::test::Client<websocketpp::config::asio_client> client;
  auto channelFuture = client.waitForChannel(TEST_PLUGIN_TOPIC);
  ASSERT_EQ(std::future_status::ready, client.connect(URI).wait_for(DEFAULT_TIMEOUT));
  ASSERT_EQ(std::future_status::ready, channelFuture.wait_for(DEFAULT_TIMEOUT));
  const auto channel = channelFuture.get();
  EXPECT_EQ("json", channel.encoding);
  EXPECT_EQ("test_plugin.Data", channel.schemaName);

  const foxglove::test::SubscriptionId subscriptionId = 1;
  auto msgFuture = client.waitForChannelMsg(subscriptionId);
  client.subscribe({{subscriptionId, channel.id}});
  ASSERT_EQ(std::future_status::ready, msgFuture.wait_for(DEFAULT_TIMEOUT));
  const auto msg = msgFuture.get();
  EXPECT_EQ(std::string(TEST_PLUGIN_MESSAGE), std::string(msg.begin(), msg.end()));

  client.unsubscribe({subscriptionId});
}

TEST(ChannelSourcePluginTest, instancesOfTheSamePluginLoadSideBySide) {
  foxglove::test::Client<websocketpp::config::asio_client> client;
  auto channelFuture = client.waitForChannel(SECOND_INSTANCE_TOPIC);
  ASSERT_EQ(std::future_status::ready, client.connect(URI).wait_for(DEFAULT_TIMEOUT));
  EXPECT_EQ(std::future_status::ready, channelFuture.wait_for(DEFAULT_TIMEOUT));
}

TEST(ChannelSourcePluginTest, failingInstancesAreSkippedWithAnError) {
  EXPECT_TRUE(contains(g_startupErrors, "instance 'throwing': initialization failure"));
  EXPECT_TRUE(contains(g_startupErrors,
                       "instance 'unknown_class': no plugin description declares "
                       "the class 'foxglove_bridge_test_plugins::DoesNotExist'"));
  EXPECT_FALSE(contains(g_startupErrors, "DoesNotExist'. If the plugin was built against"));
  EXPECT_TRUE(contains(g_startupErrors, "parameter 'no_class.plugin' is not set"));
  EXPECT_TRUE(contains(g_startupErrors, "instance 'version_mismatch'"));
  EXPECT_TRUE(contains(g_startupErrors, "rebuild it"));
  EXPECT_TRUE(
    contains(g_startupErrors, "parameter 'test_plugin.plugin' has already been declared"));
  EXPECT_EQ(5u, g_startupErrors.size());
}

TEST(ChannelSourcePluginTest, pluginsAreShutDownWithTheBridge) {
  clearInfoLogs();
  {
    foxglove_bridge::FoxgloveBridge bridge(probeBridgeOptions("shutdown_test_bridge"));
    EXPECT_FALSE(infoLogged("Test plugin instance 'probe' shut down"));
  }
  EXPECT_TRUE(infoLogged("Test plugin instance 'probe' shut down"));
}

int main(int argc, char** argv) {
  testing::InitGoogleTest(&argc, argv);
  rclcpp::init(argc, argv);

  g_defaultLogHandler = rcutils_logging_get_output_handler();
  rcutils_logging_set_output_handler(captureLogs);

  int testResult = 0;
  {
    rclcpp::executors::SingleThreadedExecutor executor;
    rclcpp::NodeOptions nodeOptions;
    nodeOptions.arguments({"--ros-args", "-r", std::string("__node:=") + BRIDGE_NODE_NAME});
    nodeOptions.append_parameter_override(foxglove_bridge::PARAM_PORT, PORT);
    // Only test_plugin and second_instance load, see failingInstancesAreSkippedWithAnError.
    nodeOptions.append_parameter_override(
      foxglove_bridge::PARAM_CHANNEL_SOURCE_PLUGINS,
      std::vector<std::string>({"throwing", "test_plugin", "unknown_class", "no_class",
                                "version_mismatch", "second_instance", "test_plugin"}));
    nodeOptions.append_parameter_override("throwing.plugin",
                                          "foxglove_bridge_test_plugins::ThrowingPlugin");
    nodeOptions.append_parameter_override("test_plugin.plugin", TEST_PLUGIN_CLASS);
    nodeOptions.append_parameter_override("unknown_class.plugin",
                                          "foxglove_bridge_test_plugins::DoesNotExist");
    nodeOptions.append_parameter_override("version_mismatch.plugin",
                                          "foxglove_bridge_test_plugins::VersionMismatchPlugin");
    nodeOptions.append_parameter_override("second_instance.plugin", TEST_PLUGIN_CLASS);
    foxglove_bridge::FoxgloveBridge node(nodeOptions);
    g_startupErrors = takeErrorLogs();
    executor.add_node(node.get_node_base_interface());

    std::thread spinnerThread([&executor]() {
      executor.spin();
    });

    testResult = RUN_ALL_TESTS();

    executor.cancel();
    spinnerThread.join();
    executor.remove_node(node.get_node_base_interface());
  }

  // The global context must be shut down before returning, see smoke_test.cpp.
  rclcpp::shutdown();
  return testResult;
}
