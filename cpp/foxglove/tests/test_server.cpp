#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
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

using Json = nlohmann::json;

using WebSocketClientInner = websocketpp::client<websocketpp::config::asio_client>;
using WebSocketConnection =
  std::shared_ptr<websocketpp::connection<websocketpp::config::asio_client>>;
using WebSocketMessage = websocketpp::config::asio_client::message_type::ptr;

namespace {

template<class T>
constexpr std::underlying_type_t<T> toUnderlying(T e) noexcept {
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

foxglove::WebSocketServer startServer(foxglove::WebSocketServerOptions&& options) {
  auto result = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(result.has_value());
  auto server = std::move(result.value());
  REQUIRE(server.port() != 0);
  return server;
}

foxglove::WebSocketServer startServer(
  foxglove::Context context,
  foxglove::WebSocketServerCapabilities capabilities = foxglove::WebSocketServerCapabilities(0),
  foxglove::WebSocketServerCallbacks&& callbacks = {},
  std::vector<std::string> supported_encodings = {}
) {
  return startServer({
    std::move(context),
    "unit-test",
    "127.0.0.1",
    0,
    std::move(callbacks),
    capabilities,
    std::move(supported_encodings),
  });
}

}  // namespace

TEST_CASE("Start and stop server") {
  auto context = foxglove::Context::create();
  auto server = startServer(context);
  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("name is not valid utf-8") {
  foxglove::WebSocketServerOptions options;
  options.name = "\x80\x80\x80\x80";
  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(!server_result.has_value());
  REQUIRE(server_result.error() == foxglove::FoxgloveError::Utf8Error);
  REQUIRE(foxglove::strerror(server_result.error()) == std::string("UTF-8 Error"));
}

TEST_CASE("we can't bind host") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "invalidhost";
  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(!server_result.has_value());
  REQUIRE(server_result.error() == foxglove::FoxgloveError::Bind);
}

TEST_CASE("supported encoding is invalid utf-8") {
  foxglove::WebSocketServerOptions options;
  options.name = "unit-test";
  options.host = "127.0.0.1";
  options.port = 0;
  options.supported_encodings.emplace_back("\x80\x80\x80\x80");
  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  REQUIRE(!server_result.has_value());
  REQUIRE(server_result.error() == foxglove::FoxgloveError::Utf8Error);
  REQUIRE(foxglove::strerror(server_result.error()) == std::string("UTF-8 Error"));
}

TEST_CASE("Log a message with and without metadata") {
  auto context = foxglove::Context::create();
  auto server = startServer(context);

  auto channel_result = foxglove::Channel::create("example", "json", std::nullopt, context);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());
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
  bool connection_opened = false;
  std::vector<uint64_t> subscribe_calls;
  std::vector<uint64_t> unsubscribe_calls;

  std::unique_lock lock{mutex};

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onSubscribe = [&](uint64_t channel_id) {
    std::scoped_lock lock{mutex};
    subscribe_calls.push_back(channel_id);
    cv.notify_all();
  };
  callbacks.onUnsubscribe = [&](uint64_t channel_id) {
    std::scoped_lock lock{mutex};
    unsubscribe_calls.push_back(channel_id);
    cv.notify_all();
  };
  auto server = startServer(context, {}, std::move(callbacks));

  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::Channel::create("example", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());

  WebSocketClient client;
  client.inner().set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connection_opened = true;
    cv.notify_all();
  });
  client.start(server.port());

  cv.wait(lock, [&] {
    return connection_opened;
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
    return !subscribe_calls.empty();
  });
  REQUIRE_THAT(subscribe_calls, Equals(std::vector<uint64_t>{1}));

  client.send(
    R"({
      "op": "unsubscribe",
      "subscriptionIds": [100]
    })"
  );
  cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return !unsubscribe_calls.empty();
  });
  REQUIRE_THAT(unsubscribe_calls, Equals(std::vector<uint64_t>{1}));
}

TEST_CASE("Capability enums") {
  REQUIRE(
    toUnderlying(foxglove::WebSocketServerCapabilities::ClientPublish) ==
    (FOXGLOVE_SERVER_CAPABILITY_CLIENT_PUBLISH)
  );
  REQUIRE(
    toUnderlying(foxglove::WebSocketServerCapabilities::ConnectionGraph) ==
    (FOXGLOVE_SERVER_CAPABILITY_CONNECTION_GRAPH)
  );
  REQUIRE(
    toUnderlying(foxglove::WebSocketServerCapabilities::Parameters) ==
    (FOXGLOVE_SERVER_CAPABILITY_PARAMETERS)
  );
  REQUIRE(
    toUnderlying(foxglove::WebSocketServerCapabilities::Time) == (FOXGLOVE_SERVER_CAPABILITY_TIME)
  );
  REQUIRE(
    toUnderlying(foxglove::WebSocketServerCapabilities::Services) ==
    (FOXGLOVE_SERVER_CAPABILITY_SERVICES)
  );
}

TEST_CASE("Client advertise/publish callbacks") {
  auto context = foxglove::Context::create();
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connection_opened = false;
  bool advertised = false;
  bool received_message = false;

  std::unique_lock lock{mutex};

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onClientAdvertise = [&](uint32_t client_id, const foxglove::ClientChannel& channel) {
    std::scoped_lock lock{mutex};
    advertised = true;
    REQUIRE(client_id == 1);
    REQUIRE(channel.id == 100);
    REQUIRE(channel.topic == "topic");
    REQUIRE(channel.encoding == "encoding");
    REQUIRE(channel.schema_name == "schema name");
    REQUIRE(channel.schema_encoding == "schema encoding");
    REQUIRE(
      std::string_view(reinterpret_cast<const char*>(channel.schema), channel.schema_len) ==
      "schema data"
    );
    cv.notify_all();
  };
  callbacks.onMessageData =
    // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
    [&](uint32_t client_id, uint32_t client_channel_id, const std::byte* data, size_t data_len) {
      std::scoped_lock lock{mutex};
      received_message = true;
      REQUIRE(client_id == 1);
      REQUIRE(data_len == 3);
      REQUIRE(char(data[0]) == 'a');
      REQUIRE(char(data[1]) == 'b');
      REQUIRE(char(data[2]) == 'c');
      cv.notify_all();
    };
  callbacks.onClientUnadvertise = [&](uint32_t client_id, uint32_t client_channel_id) {
    std::scoped_lock lock{mutex};
    advertised = false;
    REQUIRE(client_id == 1);
    REQUIRE(client_channel_id == 100);
    cv.notify_all();
  };
  auto server = startServer(
    context,
    foxglove::WebSocketServerCapabilities::ClientPublish,
    std::move(callbacks),
    {"schema encoding", "another"}
  );

  WebSocketClient client;
  client.inner().set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connection_opened = true;
    cv.notify_all();
  });
  client.start(server.port());

  cv.wait(lock, [&] {
    return connection_opened;
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
  auto advertised_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return advertised;
  });
  REQUIRE(advertised_result);

  // send ClientMessageData message
  std::array<char, 8> msg = {1, 100, 0, 0, 0, 'a', 'b', 'c'};
  client.send(msg.data(), msg.size());
  auto received_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return received_message;
  });
  REQUIRE(received_result);

  client.send(R"({ "op": "unadvertise", "channelIds": [100] })");
  cv.wait(lock, [&] {
    return !advertised;
  });
}

TEST_CASE("Parameter callbacks") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connection_opened = false;
  std::optional<std::pair<std::optional<std::string>, std::vector<std::string>>>
    server_get_parameters;
  std::optional<std::pair<std::optional<std::string>, std::vector<foxglove::Parameter>>>
    server_set_parameters;
  std::queue<std::string> client_rx;

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onGetParameters = [&](
                                uint32_t client_id,
                                std::optional<std::string_view>
                                  request_id,
                                const std::vector<std::string_view>& param_names
                              ) -> std::vector<foxglove::Parameter> {
    std::scoped_lock lock{mutex};
    std::optional<std::string> owned_request_id;
    if (request_id.has_value()) {
      owned_request_id.emplace(*request_id);
    }
    std::vector<std::string> owned_param_names;
    owned_param_names.reserve(param_names.size());
    for (const auto& name : param_names) {
      owned_param_names.emplace_back(name);
    }
    server_get_parameters = std::make_pair(owned_request_id, owned_param_names);
    cv.notify_one();
    std::vector<foxglove::Parameter> result;
    result.emplace_back("foo");
    result.emplace_back("bar", "BAR");
    result.emplace_back("baz", 1.234);
    return result;
  };
  callbacks.onSetParameters = [&](
                                uint32_t client_id,
                                std::optional<std::string_view>
                                  request_id,
                                const std::vector<foxglove::ParameterView>& params
                              ) -> std::vector<foxglove::Parameter> {
    std::scoped_lock lock{mutex};
    std::optional<std::string> owned_request_id;
    if (request_id.has_value()) {
      owned_request_id.emplace(*request_id);
    }
    std::vector<foxglove::Parameter> owned_params;
    owned_params.reserve(params.size());
    for (const auto& param : params) {
      owned_params.emplace_back(std::move(param.clone()));
    }
    server_set_parameters = std::make_pair(owned_request_id, std::move(owned_params));
    cv.notify_one();
    std::array<uint8_t, 6> data{115, 101, 99, 114, 101, 116};
    std::vector<foxglove::Parameter> result;
    result.emplace_back("zip");
    result.emplace_back("bar", 99.99);
    result.emplace_back("bytes", data.data(), data.size());
    return result;
  };
  auto context = foxglove::Context::create();
  auto server =
    startServer(context, foxglove::WebSocketServerCapabilities::Parameters, std::move(callbacks));

  WebSocketClient client;
  client.inner().set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connection_opened = true;
    cv.notify_one();
  });
  client.inner().set_message_handler(
    [&](const websocketpp::connection_hdl&, const WebSocketMessage& msg) {
      std::scoped_lock lock{mutex};
      client_rx.push(msg->get_payload());
      cv.notify_one();
    }
  );
  client.start(server.port());

  // Wait for the connection to be opened.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return connection_opened;
    });
    REQUIRE(wait_result);
  }

  // Wait for the the serverInfo message.
  std::string payload;
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !client_rx.empty();
    });
    REQUIRE(wait_result);
    payload = client_rx.front();
    client_rx.pop();
  }
  Json parsed = Json::parse(payload);
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
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (server_get_parameters.has_value()) {
        auto request_id = (*server_get_parameters).first;
        auto param_names = (*server_get_parameters).second;
        REQUIRE(request_id.has_value());
        REQUIRE(*request_id == "get-request");
        REQUIRE(param_names.size() == 4);
        REQUIRE(param_names[0] == "foo");
        REQUIRE(param_names[1] == "bar");
        REQUIRE(param_names[2] == "baz");
        REQUIRE(param_names[3] == "xxx");
        return true;
      }
      return false;
    });
    REQUIRE(wait_result);
  }

  // Wait for the response and validate it.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !client_rx.empty();
    });
    REQUIRE(wait_result);
    payload = client_rx.front();
    client_rx.pop();
  }
  parsed = Json::parse(payload);
  auto expected = Json::parse(R"({
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
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (server_set_parameters.has_value()) {
        auto [requestId, params] = *std::move(server_set_parameters);
        REQUIRE(requestId.has_value());
        REQUIRE(*requestId == "set-request");
        REQUIRE(params.size() == 3);
        REQUIRE(params[0].name() == "zip");
        REQUIRE(!params[0].value().has_value());
        REQUIRE(params[1].name() == "bar");
        REQUIRE(params[1].value().has_value());
        if (params[1].is<double>()) {
          REQUIRE(params[1].get<double>() == 99.99);
        }
        REQUIRE(params[2].name() == "bytes");
        REQUIRE(params[2].type() == foxglove::ParameterType::ByteArray);
        REQUIRE(params[2].value().has_value());
        if (params[2].isByteArray()) {
          auto result = params[2].getByteArray();
          REQUIRE(result.has_value());
          auto bytes = result.value();
          REQUIRE(bytes.size() == 6);
          REQUIRE(memcmp(bytes.data(), "secret", 6) == 0);
        }
        return true;
      }
      return false;
    });
    REQUIRE(wait_result);
  }

  // Wait for the response and validate it.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !client_rx.empty();
    });
    REQUIRE(wait_result);
    payload = client_rx.front();
    client_rx.pop();
  }
  parsed = Json::parse(payload);
  expected = Json::parse(R"({
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

TEST_CASE("Parameter subscription callbacks") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  bool connection_opened = false;
  std::optional<std::vector<std::string>> server_sub_names;
  std::optional<std::vector<std::string>> server_unsub_names;
  std::queue<std::string> client_rx;

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onParametersSubscribe = [&](const std::vector<std::string>& names) {
    std::scoped_lock lock{mutex};
    server_sub_names = names;
    cv.notify_one();
  };
  callbacks.onParametersUnsubscribe = [&](const std::vector<std::string>& names) {
    std::scoped_lock lock{mutex};
    server_unsub_names = names;
    cv.notify_one();
  };
  auto context = foxglove::Context::create();
  auto server =
    startServer(context, foxglove::WebSocketServerCapabilities::Parameters, std::move(callbacks));

  WebSocketClient client;
  client.inner().set_open_handler([&](const auto& hdl) {
    std::scoped_lock lock{mutex};
    connection_opened = true;
    cv.notify_one();
  });
  client.inner().set_message_handler(
    [&](const websocketpp::connection_hdl&, const WebSocketMessage& msg) {
      std::scoped_lock lock{mutex};
      client_rx.push(msg->get_payload());
      cv.notify_one();
    }
  );
  client.start(server.port());

  // Wait for the connection to be opened.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return connection_opened;
    });
    REQUIRE(wait_result);
  }

  // Wait for the the serverInfo message.
  std::string payload;
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !client_rx.empty();
    });
    REQUIRE(wait_result);
    payload = client_rx.front();
    client_rx.pop();
  }
  Json parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  // Send subscribeParameterUpdates.
  client.send(
    R"({
      "op": "subscribeParameterUpdates",
      "parameterNames": ["foo", "beep"]
    })"
  );

  // Wait for the server to process the callback.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (server_sub_names.has_value()) {
        auto names = *server_sub_names;
        REQUIRE_THAT(names, Equals(std::vector<std::string>{"foo", "beep"}));
        return true;
      }
      return false;
    });
    REQUIRE(wait_result);
  }

  // Send a parameter update from the server, including some parameters that we
  // expect to be filtered out, since they aren't subscribed.
  std::vector<foxglove::Parameter> params;
  params.emplace_back("baz", 1.234);
  params.emplace_back("beep", "boop");
  server.publishParameterValues(std::move(params));

  // Wait for the server to send the parameterValues message and validate it.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return !client_rx.empty();
    });
    REQUIRE(wait_result);
    payload = client_rx.front();
    client_rx.pop();
  }
  parsed = Json::parse(payload);
  auto expected = Json::parse(R"({
      "op": "parameterValues",
      "parameters": [{ "name": "beep", "value": "boop" }]
    })");
  REQUIRE(parsed == expected);
}

TEST_CASE("Publish a connection graph") {
  auto context = foxglove::Context::create();
  auto server = startServer(context, foxglove::WebSocketServerCapabilities::ConnectionGraph);
  foxglove::ConnectionGraph graph;
  graph.setPublishedTopic("topic", {"publisher1", "publisher2"});
  graph.setSubscribedTopic("topic", {"subscriber1", "subscriber2"});
  graph.setAdvertisedService("service", {"provider1", "provider2"});
  server.publishConnectionGraph(graph);
}
