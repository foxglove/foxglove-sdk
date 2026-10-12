#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/remote_access.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <chrono>
#include <fstream>
#include <iterator>
#include <optional>
#include <random>
#include <string>
#include <thread>
#include <utility>

#include "common/file_cleanup.hpp"
#include "common/test_helpers.hpp"

using Catch::Matchers::ContainsSubstring;
using foxglove_tests::FileCleanup;
using foxglove_tests::requireValue;

namespace {

/// Runs a gateway that cannot reach the Foxglove API, so it stays in the connecting state, and
/// returns the contents of an uncompressed MCAP file recorded from its context.
std::string recordGateway(std::optional<foxglove::RemoteAccessStatsOptions> stats) {
  FileCleanup file("test_remote_access_" + std::to_string(std::random_device{}()) + ".mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions mcap_options;
  mcap_options.context = context;
  mcap_options.path = file.path();
  mcap_options.compression = foxglove::McapCompression::None;
  auto writer = foxglove::McapWriter::create(mcap_options);
  REQUIRE(writer.has_value());

  foxglove::RemoteAccessGatewayOptions options;
  options.context = context;
  options.device_token = "test-token";
  // Nothing listens on this port.
  options.foxglove_api_url = "http://127.0.0.1:1";
  options.stats = std::move(stats);
  auto gateway = foxglove::RemoteAccessGateway::create(std::move(options));
  REQUIRE(gateway.has_value());

  std::this_thread::sleep_for(std::chrono::milliseconds(350));
  REQUIRE(requireValue(gateway).stop() == foxglove::FoxgloveError::Ok);
  REQUIRE(requireValue(writer).close() == foxglove::FoxgloveError::Ok);

  std::ifstream stream(file.path(), std::ios::binary);
  REQUIRE(stream.is_open());
  return {std::istreambuf_iterator<char>(stream), std::istreambuf_iterator<char>()};
}

}  // namespace

TEST_CASE("RemoteAccessGateway publishes stats while connecting and when stopped") {
  foxglove::RemoteAccessStatsOptions stats;
  stats.topic = "/gateway_stats";
  stats.interval = std::chrono::milliseconds(100);

  auto content = recordGateway(std::move(stats));

  REQUIRE_THAT(content, ContainsSubstring("/gateway_stats"));
  REQUIRE_THAT(content, ContainsSubstring("foxglove.RemoteAccessStats"));
  REQUIRE_THAT(content, ContainsSubstring(R"({"state":"connecting","session_active":false)"));
  REQUIRE_THAT(content, ContainsSubstring(R"({"state":"shutting_down","session_active":false)"));
  REQUIRE_THAT(content, !ContainsSubstring("/remote_access_stats"));
}

TEST_CASE("RemoteAccessGateway publishes stats on the default topic") {
  auto content = recordGateway(foxglove::RemoteAccessStatsOptions{});

  REQUIRE_THAT(content, ContainsSubstring("/remote_access_stats"));
  REQUIRE_THAT(content, ContainsSubstring(R"({"state":"shutting_down")"));
}

TEST_CASE("RemoteAccessGateway does not publish stats unless enabled") {
  auto content = recordGateway(std::nullopt);

  REQUIRE_THAT(content, !ContainsSubstring("foxglove.RemoteAccessStats"));
}
