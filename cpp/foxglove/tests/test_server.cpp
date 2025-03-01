#include <foxglove/channel.hpp>
#include <foxglove/server.hpp>

#include <catch2/catch_test_macros.hpp>

TEST_CASE("Start and stop server") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  foxglove::WebSocketServer server{options};
  REQUIRE(server.port() != 0);
  server.stop();
}

TEST_CASE("Log a message") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  foxglove::WebSocketServer server{options};
  REQUIRE(server.port() != 0);

  foxglove::Channel channel{"example", "json", std::nullopt};
  const uint8_t data[] = {1, 2, 3};
  channel.log(reinterpret_cast<const std::byte*>(data), sizeof(data), 1, 2, 3);
}
