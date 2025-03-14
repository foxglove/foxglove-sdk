#include <foxglove/channel.hpp>
#include <foxglove/server.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_vector.hpp>
#include <websocketpp/client.hpp>
#include <websocketpp/config/asio_no_tls_client.hpp>

using Catch::Matchers::Equals;

using WebSocketClient = websocketpp::client<websocketpp::config::asio_client>;

TEST_CASE("Start and stop server") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  foxglove::WebSocketServer server{options};
  REQUIRE(server.port() != 0);
  server.stop();
}

TEST_CASE("Log a message with and without metadata") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  foxglove::WebSocketServer server{options};
  REQUIRE(server.port() != 0);

  foxglove::Channel channel{"example", "json", std::nullopt};
  const std::array<uint8_t, 3> data = {1, 2, 3};
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size(), 1, 2, 3);
}

TEST_CASE("Subscribe and unsubscribe callbacks") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connectionOpened = false;
  std::vector<uint64_t> subscribeCalls;
  std::vector<uint64_t> unsubscribeCalls;

  std::unique_lock lock{mutex};

  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  options.callbacks.onSubscribe = [&](uint64_t channel_id) {
    std::scoped_lock lock{mutex};
    subscribeCalls.push_back(channel_id);
    cv.notify_all();
  };
  options.callbacks.onUnsubscribe = [&](uint64_t channel_id) {
    std::scoped_lock lock{mutex};
    unsubscribeCalls.push_back(channel_id);
    cv.notify_all();
  };
  foxglove::WebSocketServer server{options};
  REQUIRE(server.port() != 0);

  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  foxglove::Channel channel{"example", "json", schema};

  WebSocketClient client;
  client.clear_access_channels(websocketpp::log::alevel::all);
  client.clear_error_channels(websocketpp::log::elevel::all);
  client.set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connectionOpened = true;
    cv.notify_all();
  });
  client.init_asio();
  std::error_code ec;
  auto connection = client.get_connection("ws://127.0.0.1:" + std::to_string(server.port()), ec);
  UNSCOPED_INFO(ec.message());
  REQUIRE(!ec);
  client.connect(connection);
  std::thread clientThread{&WebSocketClient::run, std::ref(client)};

  cv.wait(lock, [&] {
    return connectionOpened;
  });
  client.send(
    connection,
    R"({
      "op": "subscribe",
      "subscriptions": [
        {
          "id": 100, "channelId": )" +
      std::to_string(channel.id()) + R"( }
      ]
    })",
    websocketpp::frame::opcode::text,
    ec
  );
  UNSCOPED_INFO(ec.message());
  REQUIRE(!ec);
  cv.wait(lock, [&] {
    return !subscribeCalls.empty();
  });
  REQUIRE_THAT(subscribeCalls, Equals(std::vector<uint64_t>{1}));

  client.send(
    connection,
    R"({
      "op": "unsubscribe",
      "subscriptionIds": [100]
    })",
    websocketpp::frame::opcode::text,
    ec
  );

  cv.wait(lock, [&] {
    return !unsubscribeCalls.empty();
  });
  REQUIRE_THAT(unsubscribeCalls, Equals(std::vector<uint64_t>{1}));

  client.close(connection, websocketpp::close::status::normal, "", ec);
  UNSCOPED_INFO(ec.message());
  REQUIRE(!ec);
  clientThread.join();
}
