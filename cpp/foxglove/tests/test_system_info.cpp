#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/system_info.hpp>

#include <catch2/catch_test_macros.hpp>

#include <chrono>
#include <thread>

#include "common/test_helpers.hpp"

using foxglove_tests::requireValue;

TEST_CASE("SystemInfoPublisher start and stop") {
  auto context = foxglove::Context::create();
  foxglove::SystemInfoOptions options;
  options.context = context;
  options.refresh_interval = std::chrono::milliseconds(200);

  auto publisher = foxglove::SystemInfoPublisher::create(std::move(options));
  REQUIRE(publisher.has_value());

  // Stopping returns Ok.
  auto& pub = requireValue(publisher);
  REQUIRE(pub.stop() == foxglove::FoxgloveError::Ok);

  // Stopping twice is a no-op.
  REQUIRE(pub.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("SystemInfoPublisher with custom topic") {
  auto context = foxglove::Context::create();
  foxglove::SystemInfoOptions options;
  options.context = context;
  options.topic = "/custom/sysinfo";
  options.refresh_interval = std::chrono::milliseconds(200);

  auto publisher = foxglove::SystemInfoPublisher::create(std::move(options));
  REQUIRE(publisher.has_value());
  REQUIRE(requireValue(publisher).stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("SystemInfoPublisher with default options") {
  // Use the default global context and default topic/refresh interval.
  auto publisher = foxglove::SystemInfoPublisher::create();
  REQUIRE(publisher.has_value());
  REQUIRE(requireValue(publisher).stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("SystemInfoPublisher destructor does not stop the publisher") {
  // The destructor detaches the background task to match Rust/Python behavior:
  // dropping the handle leaves the publisher running. We can't easily observe
  // the still-running task here, but we can at least verify destruction does
  // not crash or block, and that the handle can go out of scope without an
  // explicit stop().
  auto context = foxglove::Context::create();
  foxglove::SystemInfoOptions options;
  options.context = context;
  options.refresh_interval = std::chrono::milliseconds(200);

  {
    auto publisher = foxglove::SystemInfoPublisher::create(std::move(options));
    REQUIRE(publisher.has_value());
  }
  std::this_thread::sleep_for(std::chrono::milliseconds(50));
}
