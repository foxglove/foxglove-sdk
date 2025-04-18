#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>
#include <catch2/matchers/catch_matchers_vector.hpp>
#include <websocketpp/client.hpp>
#include <websocketpp/config/asio_no_tls_client.hpp>

#include <type_traits>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

using WebSocketClient = websocketpp::client<websocketpp::config::asio_client>;

namespace {

template<class T>
constexpr std::underlying_type_t<T> to_underlying(T e) noexcept {
  return static_cast<std::underlying_type_t<T>>(e);
}

}  // namespace

TEST_CASE("Start and stop server") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);
  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("name is not valid utf-8") {
  foxglove::WebSocketServerOptions options;
  options.name = "\x80\x80\x80\x80";
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(!serverResult.has_value());
  REQUIRE(serverResult.error() == foxglove::FoxgloveError::Utf8Error);
  REQUIRE(foxglove::strerror(serverResult.error()) == std::string("UTF-8 Error"));
}

TEST_CASE("we can't bind host") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "invalidhost";
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(!serverResult.has_value());
  REQUIRE(serverResult.error() == foxglove::FoxgloveError::Bind);
}

TEST_CASE("supported encoding is invalid utf-8") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  options.supportedEncodings.emplace_back("\x80\x80\x80\x80");
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(!serverResult.has_value());
  REQUIRE(serverResult.error() == foxglove::FoxgloveError::Utf8Error);
  REQUIRE(foxglove::strerror(serverResult.error()) == std::string("UTF-8 Error"));
}

TEST_CASE("Log a message with and without metadata") {
  auto context = foxglove::Context::create();
  foxglove::WebSocketServerOptions options{context : context};
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);

  auto channelResult = foxglove::Channel::create("example", "json", std::nullopt, context);
  REQUIRE(channelResult.has_value());
  auto channel = std::move(channelResult.value());
  const std::array<uint8_t, 3> data = {1, 2, 3};
  REQUIRE(
    channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size()) ==
    foxglove::FoxgloveError::Ok
  );
  REQUIRE(
    channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size(), 1) ==
    foxglove::FoxgloveError::Ok
  );
}

TEST_CASE("Subscribe and unsubscribe callbacks") {
  auto context = foxglove::Context::create();
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connectionOpened = false;
  std::vector<uint64_t> subscribeCalls;
  std::vector<uint64_t> unsubscribeCalls;

  std::unique_lock lock{mutex};

  foxglove::WebSocketServerOptions options{context : context};
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
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);

  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channelResult = foxglove::Channel::create("example", "json", schema, context);
  REQUIRE(channelResult.has_value());
  auto channel = std::move(channelResult.value());

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
  connection->add_subprotocol("foxglove.sdk.v1");
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
  cv.wait_for(lock, std::chrono::seconds(1), [&] {
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
  cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return !unsubscribeCalls.empty();
  });
  REQUIRE_THAT(unsubscribeCalls, Equals(std::vector<uint64_t>{1}));

  client.close(connection, websocketpp::close::status::normal, "", ec);
  UNSCOPED_INFO(ec.message());
  REQUIRE(!ec);
  clientThread.join();
}

TEST_CASE("Capability enums") {
  REQUIRE(
    to_underlying(foxglove::WebSocketServerCapabilities::ClientPublish) ==
    (FOXGLOVE_SERVER_CAPABILITY_CLIENT_PUBLISH)
  );
  REQUIRE(
    to_underlying(foxglove::WebSocketServerCapabilities::ConnectionGraph) ==
    (FOXGLOVE_SERVER_CAPABILITY_CONNECTION_GRAPH)
  );
  REQUIRE(
    to_underlying(foxglove::WebSocketServerCapabilities::Parameters) ==
    (FOXGLOVE_SERVER_CAPABILITY_PARAMETERS)
  );
  REQUIRE(
    to_underlying(foxglove::WebSocketServerCapabilities::Time) == (FOXGLOVE_SERVER_CAPABILITY_TIME)
  );
  REQUIRE(
    to_underlying(foxglove::WebSocketServerCapabilities::Services) ==
    (FOXGLOVE_SERVER_CAPABILITY_SERVICES)
  );
}

TEST_CASE("Client advertise/publish callbacks") {
  auto context = foxglove::Context::create();
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connectionOpened = false;
  bool advertised = false;
  bool receivedMessage = false;

  std::unique_lock lock{mutex};

  foxglove::WebSocketServerOptions options{context : context};
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  options.capabilities = foxglove::WebSocketServerCapabilities::ClientPublish;
  options.callbacks.onClientAdvertise =
    [&](uint32_t clientId, const foxglove::ClientChannel& channel) {
      std::scoped_lock lock{mutex};
      advertised = true;
      REQUIRE(clientId == 1);
      REQUIRE(channel.id == 100);
      REQUIRE(channel.topic == "topic");
      REQUIRE(channel.encoding == "encoding");
      REQUIRE(channel.schemaName == "schema name");
      REQUIRE(channel.schemaEncoding == "schema encoding");
      REQUIRE(
        std::string_view(reinterpret_cast<const char*>(channel.schema), channel.schemaLen) ==
        "schema data"
      );
      cv.notify_all();
    };
  options.callbacks.onMessageData =
    // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
    [&](uint32_t clientId, uint32_t clientChannelId, const std::byte* data, size_t dataLen) {
      std::scoped_lock lock{mutex};
      receivedMessage = true;
      REQUIRE(clientId == 1);
      REQUIRE(dataLen == 3);
      REQUIRE(char(data[0]) == 'a');
      REQUIRE(char(data[1]) == 'b');
      REQUIRE(char(data[2]) == 'c');
      cv.notify_all();
    };
  options.callbacks.onClientUnadvertise = [&](uint32_t clientId, uint32_t clientChannelId) {
    std::scoped_lock lock{mutex};
    advertised = false;
    REQUIRE(clientId == 1);
    REQUIRE(clientChannelId == 100);
    cv.notify_all();
  };
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);

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
  connection->add_subprotocol("foxglove.sdk.v1");
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
      "op": "advertise",
      "channels": [
        {
          "id": 100,
          "topic": "topic",
          "encoding": "encoding",
          "schemaName": "schema name",
          "schemaEncoding": "schema encoding",
          "schema": "schema data"
        }
      ]
    })",
    websocketpp::frame::opcode::text,
    ec
  );
  UNSCOPED_INFO(ec.message());
  REQUIRE(!ec);
  auto advertisedResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return advertised;
  });
  REQUIRE(advertisedResult);

  // send ClientMessageData message
  std::array<char, 8> msg = {1, 100, 0, 0, 0, 'a', 'b', 'c'};
  client.send(connection, msg.data(), msg.size(), websocketpp::frame::opcode::binary, ec);
  auto receivedResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return receivedMessage;
  });
  REQUIRE(receivedResult);

  client.send(
    connection,
    R"({
      "op": "unadvertise",
      "channelIds": [100]
    })",
    websocketpp::frame::opcode::text,
    ec
  );
  cv.wait(lock, [&] {
    return !advertised;
  });

  client.close(connection, websocketpp::close::status::normal, "", ec);
  UNSCOPED_INFO(ec.message());
  REQUIRE(!ec);
  clientThread.join();
}
