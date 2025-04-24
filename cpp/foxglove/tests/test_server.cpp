#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>
#include <catch2/matchers/catch_matchers_vector.hpp>
#include <nlohmann/json.hpp>
#include <websocketpp/client.hpp>
#include <websocketpp/config/asio_no_tls_client.hpp>

#include <type_traits>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;

using json = nlohmann::json;

using WebSocketClientInner = websocketpp::client<websocketpp::config::asio_client>;
using WebSocketConnection =
  std::shared_ptr<websocketpp::connection<websocketpp::config::asio_client>>;
using WebSocketMessage = websocketpp::config::asio_client::message_type::ptr;

namespace {

template<class T>
constexpr std::underlying_type_t<T> to_underlying(T e) noexcept {
  return static_cast<std::underlying_type_t<T>>(e);
}

class WebSocketClient {
public:
  explicit WebSocketClient() {
    client_.clear_access_channels(websocketpp::log::alevel::all);
    client_.clear_error_channels(websocketpp::log::elevel::all);
    client_.init_asio();
  }

  WebSocketClient(const WebSocketClient&) = delete;
  WebSocketClient(WebSocketClient&&) = delete;
  WebSocketClient& operator=(const WebSocketClient&) = delete;
  WebSocketClient& operator=(WebSocketClient&&) = delete;

  ~WebSocketClient() {
    if (!started_ || !thread_.joinable()) {
      return;
    }
    if (!closed_) {
      std::error_code ec;
      client_.close(connection_, websocketpp::close::status::normal, "", ec);
      UNSCOPED_INFO(ec.message());
    }
    client_.stop();
    std::error_code ec;
    thread_.join();
  }

  void start(uint16_t port) {
    std::error_code ec;
    connection_ = client_.get_connection("ws://127.0.0.1:" + std::to_string(port), ec);
    connection_->add_subprotocol("foxglove.sdk.v1");
    UNSCOPED_INFO(ec.message());
    REQUIRE(!ec);
    client_.connect(connection_);
    started_ = true;
    thread_ = std::thread{&WebSocketClientInner::run, std::ref(client_)};
  }

  void send(std::string const& payload) {
    std::error_code ec;
    client_.send(connection_, payload, websocketpp::frame::opcode::text, ec);
    UNSCOPED_INFO(ec.message());
    REQUIRE(!ec);
  }

  void send(void const* payload, size_t len) {
    std::error_code ec;
    client_.send(connection_, payload, len, websocketpp::frame::opcode::binary, ec);
    UNSCOPED_INFO(ec.message());
    REQUIRE(!ec);
  }

  void close() {
    closed_ = true;
    std::error_code ec;
    client_.close(connection_, websocketpp::close::status::normal, "", ec);
    UNSCOPED_INFO(ec.message());
    REQUIRE(!ec);
  }

  WebSocketClientInner& inner() {
    return client_;
  }

private:
  WebSocketClientInner client_;
  WebSocketConnection connection_;
  std::thread thread_;
  bool started_{};
  bool closed_{};
};

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
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);

  auto channelResult = foxglove::Channel::create("example", "json", std::nullopt);
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
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);

  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channelResult = foxglove::Channel::create("example", "json", schema);
  REQUIRE(channelResult.has_value());
  auto channel = std::move(channelResult.value());

  WebSocketClient client;
  client.inner().set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connectionOpened = true;
    cv.notify_all();
  });
  client.start(server.port());

  cv.wait(lock, [&] {
    return connectionOpened;
  });
  client.send(
    R"({
      "op": "subscribe",
      "subscriptions": [
        {
          "id": 100, "channelId": )" +
    std::to_string(channel.id()) + R"( }
      ]
    })"
  );
  cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return !subscribeCalls.empty();
  });
  REQUIRE_THAT(subscribeCalls, Equals(std::vector<uint64_t>{1}));

  client.send(
    R"({
      "op": "unsubscribe",
      "subscriptionIds": [100]
    })"
  );
  cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return !unsubscribeCalls.empty();
  });
  REQUIRE_THAT(unsubscribeCalls, Equals(std::vector<uint64_t>{1}));
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
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connectionOpened = false;
  bool advertised = false;
  bool receivedMessage = false;

  std::unique_lock lock{mutex};

  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  options.capabilities = foxglove::WebSocketServerCapabilities::ClientPublish;
  options.supportedEncodings = {"schema encoding", "another"};
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
  client.inner().set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connectionOpened = true;
    cv.notify_all();
  });
  client.start(server.port());

  cv.wait(lock, [&] {
    return connectionOpened;
  });
  client.send(
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
    })"
  );
  auto advertisedResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return advertised;
  });
  REQUIRE(advertisedResult);

  // send ClientMessageData message
  std::array<char, 8> msg = {1, 100, 0, 0, 0, 'a', 'b', 'c'};
  client.send(msg.data(), msg.size());
  auto receivedResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return receivedMessage;
  });
  REQUIRE(receivedResult);

  client.send(R"({ "op": "unadvertise", "channelIds": [100] })");
  cv.wait(lock, [&] {
    return !advertised;
  });
}

TEST_CASE("Parameter callbacks") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connectionOpened = false;
  std::optional<std::pair<std::string, std::vector<std::string>>> serverGetParameters;
  std::optional<std::pair<std::string, std::vector<foxglove::Parameter>>> serverSetParameters;
  std::queue<std::string> clientRx;

  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  options.capabilities = foxglove::WebSocketServerCapabilities::Parameters;
  options.callbacks.onGetParameters =
    [&](
      uint32_t clientId, std::string_view requestId, const std::vector<std::string>& paramNames
    ) -> std::vector<foxglove::Parameter> {
    std::scoped_lock lock{mutex};
    serverGetParameters = std::make_pair(requestId, paramNames);
    cv.notify_one();
    std::vector<foxglove::Parameter> result;
    result.emplace_back("foo");
    result.emplace_back("bar", "BAR");
    result.emplace_back("baz", 1.234);
    return result;
  };
  options.callbacks.onSetParameters = [&](
                                        uint32_t clientId,
                                        std::string_view requestId,
                                        const std::vector<foxglove::ParameterView>& params
                                      ) -> std::vector<foxglove::Parameter> {
    std::scoped_lock lock{mutex};
    std::vector<foxglove::Parameter> ownedParams;
    ownedParams.reserve(params.size());
    for (const auto& param : params) {
      ownedParams.emplace_back(std::move(param.clone()));
    }
    serverSetParameters = std::make_pair(requestId, std::move(ownedParams));
    cv.notify_one();
    std::array<uint8_t, 6> data{115, 101, 99, 114, 101, 116};
    std::vector<foxglove::Parameter> result;
    result.emplace_back("zip");
    result.emplace_back("bar", 99.99);
    result.emplace_back("bytes", data.data(), data.size());
    return result;
  };
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);

  WebSocketClient client;
  client.inner().set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connectionOpened = true;
    cv.notify_one();
  });
  client.inner().set_message_handler(
    [&](const websocketpp::connection_hdl&, const WebSocketMessage& msg) {
      std::scoped_lock lock{mutex};
      clientRx.push(msg->get_payload());
      cv.notify_one();
    }
  );
  client.start(server.port());

  // Wait for the connection to be opened.
  {
    std::unique_lock lock{mutex};
    auto waitResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return connectionOpened;
    });
    REQUIRE(waitResult);
  }

  // Wait for the the serverInfo message.
  std::string payload;
  {
    std::unique_lock lock{mutex};
    auto waitResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !clientRx.empty();
    });
    REQUIRE(waitResult);
    payload = clientRx.front();
    clientRx.pop();
  }
  json parsed = json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  // Send getParameters.
  client.send(
    R"({
      "op": "getParameters",
      "id": "get-request",
      "parameterNames": [ "foo", "bar", "baz", "xxx" ]
    })"
  );

  // Wait for the server to process the callback.
  {
    std::unique_lock lock{mutex};
    auto waitResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (serverGetParameters.has_value()) {
        auto requestId = (*serverGetParameters).first;
        auto paramNames = (*serverGetParameters).second;
        REQUIRE(requestId == "get-request");
        REQUIRE(paramNames.size() == 4);
        REQUIRE(paramNames[0] == "foo");
        REQUIRE(paramNames[1] == "bar");
        REQUIRE(paramNames[2] == "baz");
        REQUIRE(paramNames[3] == "xxx");
        return true;
      }
      return false;
    });
    REQUIRE(waitResult);
  }

  // Wait for the response and validate it.
  {
    std::unique_lock lock{mutex};
    auto waitResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !clientRx.empty();
    });
    REQUIRE(waitResult);
    payload = clientRx.front();
    clientRx.pop();
  }
  parsed = json::parse(payload);
  auto expected = json::parse(R"({
      "op": "parameterValues",
      "id": "get-request",
      "parameters": [
        { "name": "foo" },
        { "name": "bar", "value": "BAR" },
        { "name": "baz", "type": "float64", "value": 1.234 }
      ]
    })");
  REQUIRE(parsed == expected);

  // Send setParameters.
  client.send(
    R"({
      "op": "setParameters",
      "id": "set-request",
      "parameters": [
        { "name": "zip" },
        { "name": "bar", "value": 99.99 },
        { "name": "bytes", "type": "byte_array", "value": "c2VjcmV0" }
      ]
    })"
  );

  // Wait for the server to process the callback.
  {
    std::unique_lock lock{mutex};
    auto waitResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (serverSetParameters.has_value()) {
        auto [requestId, params] = *std::move(serverSetParameters);
        REQUIRE(requestId == "set-request");
        REQUIRE(params.size() == 3);
        REQUIRE(params[0].name() == "zip");
        REQUIRE(!params[0].value().has_value());
        REQUIRE(params[1].name() == "bar");
        REQUIRE(params[1].value().has_value());
        if (params[1].value().has_value()) {
          REQUIRE(std::holds_alternative<double>(*params[1].value()));
          REQUIRE(std::get<double>(*params[1].value()) == 99.99);
        }
        REQUIRE(params[2].name() == "bytes");
        REQUIRE(params[2].type() == foxglove::ParameterType::ByteArray);
        REQUIRE(params[2].value().has_value());
        if (params[2].value().has_value()) {
          REQUIRE(std::holds_alternative<std::string>(*params[2].value()));
          REQUIRE(std::get<std::string>(*params[2].value()) == "c2VjcmV0");
        }
        return true;
      }
      return false;
    });
    REQUIRE(waitResult);
  }

  // Wait for the response and validate it.
  {
    std::unique_lock lock{mutex};
    auto waitResult = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !clientRx.empty();
    });
    REQUIRE(waitResult);
    payload = clientRx.front();
    clientRx.pop();
  }
  parsed = json::parse(payload);
  expected = json::parse(R"({
      "op": "parameterValues",
      "id": "set-request",
      "parameters": [
        { "name": "zip" },
        { "name": "bar", "type": "float64", "value": 99.99 },
        { "name": "bytes", "type": "byte_array", "value": "c2VjcmV0" }
      ]
    })");
  REQUIRE(parsed == expected);
}

TEST_CASE("Publish a connection graph") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  options.capabilities = foxglove::WebSocketServerCapabilities::ConnectionGraph;
  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(serverResult.has_value());
  auto& server = serverResult.value();
  REQUIRE(server.port() != 0);

  foxglove::ConnectionGraph graph;
  graph.setPublishedTopic("topic", {"publisher1", "publisher2"});
  graph.setSubscribedTopic("topic", {"subscriber1", "subscriber2"});
  graph.setAdvertisedService("service", {"provider1", "provider2"});
  server.publishConnectionGraph(graph);

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}
