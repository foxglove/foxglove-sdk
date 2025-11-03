#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/player_state.hpp>
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

using namespace std::string_literals;
using namespace std::string_view_literals;

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
    client_.set_open_handler([this](const auto& hdl [[maybe_unused]]) {
      std::scoped_lock lock{mutex_};
      connection_opened_ = true;
      cv_.notify_one();
    });
    client_.set_message_handler(
      [this](const websocketpp::connection_hdl&, const WebSocketMessage& msg) {
        std::scoped_lock lock{mutex_};
        rx_queue_.push(msg->get_payload());
        cv_.notify_one();
      }
    );
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

  void waitForConnection() {
    std::unique_lock lock{mutex_};
    auto wait_result = cv_.wait_for(lock, std::chrono::seconds(1), [this] {
      return connection_opened_;
    });
    REQUIRE(wait_result);
  }

  std::string recv() {
    std::unique_lock lock{mutex_};
    auto wait_result = cv_.wait_for(lock, std::chrono::seconds(1), [this] {
      return !rx_queue_.empty();
    });
    REQUIRE(wait_result);
    std::string payload = rx_queue_.front();
    rx_queue_.pop();
    return payload;
  }

  template<typename Predicate>
  std::optional<std::string> filterRecv(
    Predicate predicate, std::chrono::milliseconds timeout = std::chrono::milliseconds(500)
  ) {
    std::unique_lock lock{mutex_};
    auto start_time = std::chrono::steady_clock::now();

    constexpr std::chrono::milliseconds single_message_timeout = std::chrono::milliseconds(100);

    while (std::chrono::steady_clock::now() - start_time < timeout) {
      auto wait_result = cv_.wait_for(lock, single_message_timeout, [this] {
        return !rx_queue_.empty();
      });
      if (wait_result) {
        std::string payload = rx_queue_.front();
        rx_queue_.pop();
        if (predicate(payload)) {
          return payload;
        }
      }
    }
    return std::nullopt;
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

  void send(std::vector<std::byte>& payload) {
    this->send(payload.data(), payload.size());
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

  std::mutex mutex_;
  std::condition_variable cv_;
  // The following members are protected by the mutex.
  bool connection_opened_{};
  std::queue<std::string> rx_queue_;
};

foxglove::WebSocketServer startServer(foxglove::WebSocketServerOptions&& options) {
  // always select an available port
  options.port = 0;
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
  foxglove::WebSocketServerOptions options;
  options.context = std::move(context);
  options.name = "unit-test";
  options.callbacks = std::move(callbacks);
  options.capabilities = capabilities;
  options.supported_encodings = std::move(supported_encodings);
  return startServer(std::move(options));
}

foxglove::WebSocketServer startServer(
  foxglove::Context context, foxglove::FetchAssetHandler&& fetch_asset
) {
  foxglove::WebSocketServerOptions options;
  options.context = std::move(context);
  options.name = "unit-test";
  options.fetch_asset = std::move(fetch_asset);
  return startServer(std::move(options));
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

  auto channel_result = foxglove::RawChannel::create("example", "json", std::nullopt, context);
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
  std::vector<uint64_t> subscribe_calls;
  std::vector<uint64_t> unsubscribe_calls;

  std::unique_lock lock{mutex};

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onSubscribe =
    [&](uint64_t channel_id, const foxglove::ClientMetadata& _ [[maybe_unused]]) {
      std::scoped_lock lock{mutex};
      subscribe_calls.push_back(channel_id);
      cv.notify_all();
    };
  callbacks.onUnsubscribe =
    [&](uint64_t channel_id, const foxglove::ClientMetadata& _ [[maybe_unused]]) {
      std::scoped_lock lock{mutex};
      unsubscribe_calls.push_back(channel_id);
      cv.notify_all();
    };
  auto server = startServer(context, {}, std::move(callbacks));

  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

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

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
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
    [&](
      uint32_t client_id [[maybe_unused]],
      uint32_t client_channel_id [[maybe_unused]],
      const std::byte* data,
      size_t data_len
    ) {
      std::scoped_lock lock{mutex};
      received_message = true;
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
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

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

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("Parameter callbacks") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  std::optional<std::pair<std::optional<std::string>, std::vector<std::string>>>
    server_get_parameters;
  std::optional<std::pair<std::optional<std::string>, std::vector<foxglove::Parameter>>>
    server_set_parameters;

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onGetParameters = [&](
                                uint32_t client_id [[maybe_unused]],
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
                                uint32_t client_id [[maybe_unused]],
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
      owned_params.emplace_back(param.clone());
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
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
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
  payload = client.recv();
  parsed = Json::parse(payload);
  auto expected = Json::parse(R"({
      "op": "parameterValues",
      "id": "get-request",
      "parameters": [
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
  payload = client.recv();
  parsed = Json::parse(payload);
  expected = Json::parse(R"({
      "op": "parameterValues",
      "id": "set-request",
      "parameters": [
        { "name": "bar", "type": "float64", "value": 99.99 },
        { "name": "bytes", "type": "byte_array", "value": "c2VjcmV0" }
      ]
    })");
  REQUIRE(parsed == expected);

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("Parameter subscription callbacks") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  std::optional<std::vector<std::string>> server_sub_names;
  std::optional<std::vector<std::string>> server_unsub_names;

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onParametersSubscribe = [&](const std::vector<std::string_view>& names) {
    std::scoped_lock lock{mutex};
    server_sub_names.emplace();
    server_sub_names->reserve(names.size());
    for (const auto& name : names) {
      server_sub_names->emplace_back(name);
    }
    cv.notify_one();
  };
  callbacks.onParametersUnsubscribe = [&](const std::vector<std::string_view>& names) {
    std::scoped_lock lock{mutex};
    server_unsub_names.emplace();
    server_unsub_names->reserve(names.size());
    for (const auto& name : names) {
      server_unsub_names->emplace_back(name);
    }
    cv.notify_one();
  };
  auto context = foxglove::Context::create();
  auto server =
    startServer(context, foxglove::WebSocketServerCapabilities::Parameters, std::move(callbacks));

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
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
  payload = client.recv();
  parsed = Json::parse(payload);
  auto expected = Json::parse(R"({
      "op": "parameterValues",
      "parameters": [{ "name": "beep", "value": "boop" }]
    })");
  REQUIRE(parsed == expected);

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
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

std::vector<std::byte> makeBytes(std::string_view sv) {
  const auto* data = reinterpret_cast<const std::byte*>(sv.data());
  return {data, data + sv.size()};
}

foxglove::ServiceMessageSchema makeServiceMessageSchema(std::string_view name) {
  static auto json_schema = makeBytes(R"({"type": "object"})");
  return foxglove::ServiceMessageSchema{
    "json"s,
    foxglove::Schema{
      std::string(name),
      "jsonschema"s,
      json_schema.data(),
      json_schema.size(),
    }
  };
}

foxglove::ServiceSchema makeServiceSchema(std::string_view name) {
  return foxglove::ServiceSchema{
    std::string(name),
    makeServiceMessageSchema("request"),
    makeServiceMessageSchema("response"),
  };
}

template<typename T>
void writeIntLE(std::vector<std::byte>& buffer, T value) {
  static_assert(std::is_integral<T>());
  for (size_t shift = 0; shift < 8 * sizeof(T); shift += 8) {
    buffer.push_back(static_cast<std::byte>((value >> shift) & 0xffU));
  }
}

void writeFloatLE(std::vector<std::byte>& buffer, float value) {
  // Put the bits into a temporary uint32_t
  uint32_t tmp = 0;
  memcpy(&tmp, &value, sizeof(tmp));
  writeIntLE(buffer, tmp);
}

uint32_t readUint32LE(const std::vector<std::byte>& buffer, size_t offset) {
  REQUIRE(offset + 4 <= buffer.size());
  return (static_cast<uint32_t>(buffer[offset + 0]) << 0) |
         (static_cast<uint32_t>(buffer[offset + 1]) << 8) |
         (static_cast<uint32_t>(buffer[offset + 2]) << 16) |
         (static_cast<uint32_t>(buffer[offset + 3]) << 24);
}

std::vector<std::byte> makeServiceRequest(
  uint32_t service_id, uint32_t call_id, std::string_view encoding,
  const std::vector<std::byte>& payload
) {
  std::vector<std::byte> buffer;
  buffer.reserve(1 + 4 + 4 + 4 + encoding.size() + payload.size());
  buffer.emplace_back(static_cast<std::byte>(2));  // Service call request opcode
  writeIntLE(buffer, service_id);
  writeIntLE(buffer, call_id);
  writeIntLE(buffer, static_cast<uint32_t>(encoding.size()));
  for (char c : encoding) {
    buffer.emplace_back(static_cast<std::byte>(c));
  }
  for (auto b : payload) {
    buffer.emplace_back(b);
  }
  return buffer;
}

void validateServiceResponse(
  // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
  const std::string_view response, uint32_t service_id, uint32_t call_id, std::string_view encoding,
  const std::vector<std::byte>& payload
) {
  std::vector<std::byte> bytes(response.size());
  std::memcpy(bytes.data(), response.data(), response.size());
  REQUIRE(response.size() >= 1 + 4 + 4 + 4);
  REQUIRE(static_cast<uint8_t>(bytes[0]) == 3);  // Service call response opcode
  REQUIRE(readUint32LE(bytes, 1) == service_id);
  REQUIRE(readUint32LE(bytes, 5) == call_id);
  REQUIRE(readUint32LE(bytes, 9) == encoding.size());
  REQUIRE(response.size() >= 13 + encoding.size());
  REQUIRE(memcmp(response.data() + 13, encoding.data(), encoding.size()) == 0);
  REQUIRE(response.size() >= 13 + encoding.size() + payload.size());
  REQUIRE(memcmp(response.data() + 13 + encoding.size(), payload.data(), payload.size()) == 0);
}

TEST_CASE("Service callbacks") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  std::optional<foxglove::ServiceRequest> last_request;

  auto context = foxglove::Context::create();
  auto server = startServer(context, foxglove::WebSocketServerCapabilities::Services, {}, {"json"});

  // Register an echo service.
  foxglove::ServiceSchema echo_schema{"echo schema"};
  foxglove::ServiceHandler echo_handler(
    [&](const foxglove::ServiceRequest& request, foxglove::ServiceResponder&& responder) {
      std::scoped_lock lock{mutex};
      last_request = request;
      std::move(responder).respondOk(request.payload);
      cv.notify_one();
    }
  );
  auto service = foxglove::Service::create("/echo", echo_schema, echo_handler);
  REQUIRE(service.has_value());
  auto error = server.addService(std::move(*service));
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Register a service with a more complicated schema that returns errors.
  foxglove::ServiceSchema error_schema = makeServiceSchema("error schema");
  foxglove::ServiceHandler error_handler(
    [&](const foxglove::ServiceRequest& request, foxglove::ServiceResponder&& responder) {
      std::scoped_lock lock{mutex};
      last_request = request;
      std::move(responder).respondError("oh noes"sv);
      cv.notify_one();
    }
  );
  service = foxglove::Service::create("/error", error_schema, error_handler);
  REQUIRE(service.has_value());
  error = server.addService(std::move(*service));
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto rx_payload = client.recv();
  auto parsed = Json::parse(rx_payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  // Wait for the service advertisement message.
  rx_payload = client.recv();
  parsed = Json::parse(rx_payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "advertiseServices");
  REQUIRE(parsed.contains("services"));
  std::map<std::string, uint32_t> service_ids;
  for (const auto& parsedService : parsed["services"]) {
    REQUIRE(parsedService.contains("id"));
    REQUIRE(parsedService.contains("name"));
    uint8_t id(parsedService["id"]);
    std::string name(parsedService["name"]);
    service_ids[name] = id;
    Json expected;
    if (name == "/echo") {
      expected = Json::parse(R"({
          "id": 0,
          "name": "/echo",
          "type": "echo schema",
          "requestSchema": "",
          "responseSchema": ""
       })");
    } else if (name == "/error") {
      expected = Json::parse(R"({
          "id": 0,
          "name": "/error",
          "type": "error schema",
          "request": {
            "encoding": "json",
            "schemaName": "request",
            "schemaEncoding": "jsonschema",
            "schema": "{\"type\": \"object\"}"
          },
          "response": {
            "encoding": "json",
            "schemaName": "response",
            "schemaEncoding": "jsonschema",
            "schema": "{\"type\": \"object\"}"
          }
       })");
    } else {
    }
    expected["id"] = id;
    REQUIRE(parsedService == expected);
  }
  REQUIRE(service_ids.count("/echo") == 1);
  REQUIRE(service_ids.count("/error") == 1);

  // Make an echo service call.
  auto request_payload = makeBytes(R"({"hello": "there"})");
  auto service_request = makeServiceRequest(service_ids["/echo"], 99, "json", request_payload);
  client.send(service_request);

  // Wait for the server to process the callback.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (last_request.has_value()) {
        REQUIRE(last_request->service_name == "/echo");
        REQUIRE(last_request->call_id == 99);
        REQUIRE(last_request->encoding == "json");
        REQUIRE(last_request->payload == request_payload);
        last_request.reset();
        return true;
      }
      return false;
    });
    REQUIRE(wait_result);
  }

  // Wait for the response.
  rx_payload = client.recv();
  validateServiceResponse(rx_payload, service_ids["/echo"], 99, "json", request_payload);

  // Make an error service call.
  service_request = makeServiceRequest(service_ids["/error"], 123, "json", request_payload);
  client.send(service_request);

  // Wait for the server to process the callback.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (last_request.has_value()) {
        REQUIRE(last_request->service_name == "/error");
        REQUIRE(last_request->call_id == 123);
        REQUIRE(last_request->encoding == "json");
        REQUIRE(last_request->payload == request_payload);
        last_request.reset();
        return true;
      }
      return false;
    });
    REQUIRE(wait_result);
  }

  // Wait for the response.
  rx_payload = client.recv();
  parsed = Json::parse(rx_payload);
  auto expected_response = Json::parse(R"({
    "op": "serviceCallFailure",
    "serviceId": 0,
    "callId": 123,
    "message": "oh noes"
  })");
  expected_response["serviceId"] = service_ids["/error"];
  REQUIRE(parsed == expected_response);

  // Remove a service.
  error = server.removeService("/error");
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Wait for the unadvertise message.
  rx_payload = client.recv();
  parsed = Json::parse(rx_payload);
  auto expected = Json::parse(R"({
    "op": "unadvertiseServices",
    "serviceIds": [1]
  })");
  expected["serviceIds"][0] = service_ids["/error"];
  REQUIRE(parsed == expected);

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

void validateFetchAssetOkResponse(
  const std::string_view response, uint32_t request_id, const std::vector<std::byte>& payload
) {
  std::vector<std::byte> bytes(response.size());
  std::memcpy(bytes.data(), response.data(), response.size());
  REQUIRE(response.size() >= 1 + 4 + 1 + 4);
  REQUIRE(static_cast<uint8_t>(bytes[0]) == 4);  // Fetch asset response opcode
  REQUIRE(readUint32LE(bytes, 1) == request_id);
  REQUIRE(bytes[5] == std::byte{0});     // Success
  REQUIRE(readUint32LE(bytes, 6) == 0);  // Error message length
  REQUIRE(response.size() >= 10 + payload.size());
  REQUIRE(memcmp(response.data() + 10, payload.data(), payload.size()) == 0);
}

TEST_CASE("Fetch asset callback") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  std::optional<std::string> last_uri;

  auto context = foxglove::Context::create();
  auto server =
    startServer(context, [&](std::string_view uri, foxglove::FetchAssetResponder&& responder) {
      std::scoped_lock lock{mutex};
      last_uri.emplace(uri);
      auto data = makeBytes("data");
      std::move(responder).respondOk(data);
      cv.notify_one();
    });

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto rx_payload = client.recv();
  auto parsed = Json::parse(rx_payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");
  REQUIRE(parsed.contains("capabilities"));
  auto capabilities = parsed["capabilities"].get<std::vector<std::string>>();
  REQUIRE(capabilities.size() == 1);
  REQUIRE(capabilities[0] == "assets");

  // Make a fetch asset call.
  client.send(R"({
      "op": "fetchAsset",
      "uri": "package://foo/robot.urdf",
      "requestId": 42
  })");

  // Wait for the server to process the callback.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (last_uri.has_value()) {
        REQUIRE(last_uri == "package://foo/robot.urdf");
        last_uri.reset();
        return true;
      }
      return false;
    });
    REQUIRE(wait_result);
  }

  // Wait for the response.
  rx_payload = client.recv();
  validateFetchAssetOkResponse(rx_payload, 42, makeBytes("data"));

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

void validateFetchAssetErrorResponse(
  const std::string_view response, uint32_t request_id, std::string_view error_message
) {
  std::vector<std::byte> bytes(response.size());
  std::memcpy(bytes.data(), response.data(), response.size());
  REQUIRE(response.size() >= 1 + 4 + 1 + 4);
  REQUIRE(static_cast<uint8_t>(bytes[0]) == 4);  // Fetch asset response opcode
  REQUIRE(readUint32LE(bytes, 1) == request_id);
  REQUIRE(bytes[5] == std::byte{1});  // Error
  REQUIRE(readUint32LE(bytes, 6) == error_message.size());
  REQUIRE(response.size() >= 10 + error_message.size());
  REQUIRE(memcmp(response.data() + 10, error_message.data(), error_message.size()) == 0);
}

TEST_CASE("Fetch asset error") {
  std::mutex mutex;
  std::condition_variable cv;
  // the following variables are protected by the mutex:
  std::optional<std::string> last_uri;

  auto context = foxglove::Context::create();
  auto server =
    startServer(context, [&](std::string_view uri, foxglove::FetchAssetResponder&& responder) {
      std::scoped_lock lock{mutex};
      last_uri.emplace(uri);
      std::move(responder).respondError("oh no");
      cv.notify_one();
    });

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto rx_payload = client.recv();
  auto parsed = Json::parse(rx_payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");
  REQUIRE(parsed.contains("capabilities"));
  auto capabilities = parsed["capabilities"].get<std::vector<std::string>>();
  REQUIRE(capabilities.size() == 1);
  REQUIRE(capabilities[0] == "assets");

  // Make a fetch asset call.
  client.send(R"({
      "op": "fetchAsset",
      "uri": "package://foo/robot.urdf",
      "requestId": 42
  })");

  // Wait for the server to process the callback.
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      if (last_uri.has_value()) {
        REQUIRE(last_uri == "package://foo/robot.urdf");
        last_uri.reset();
        return true;
      }
      return false;
    });
    REQUIRE(wait_result);
  }

  // Wait for the response.
  rx_payload = client.recv();
  validateFetchAssetErrorResponse(rx_payload, 42, "oh no"sv);

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

uint64_t readUint64LE(const std::vector<std::byte>& buffer, size_t offset) {
  REQUIRE(offset + 8 <= buffer.size());
  return (static_cast<uint64_t>(buffer[offset + 0]) << 0) |
         (static_cast<uint64_t>(buffer[offset + 1]) << 8) |
         (static_cast<uint64_t>(buffer[offset + 2]) << 16) |
         (static_cast<uint64_t>(buffer[offset + 3]) << 24) |
         (static_cast<uint64_t>(buffer[offset + 4]) << 32) |
         (static_cast<uint64_t>(buffer[offset + 5]) << 40) |
         (static_cast<uint64_t>(buffer[offset + 6]) << 48) |
         (static_cast<uint64_t>(buffer[offset + 7]) << 56);
}

void validateTimeMessage(const std::string_view msg, uint64_t timestamp) {
  std::vector<std::byte> bytes(msg.size());
  std::memcpy(bytes.data(), msg.data(), msg.size());
  REQUIRE(msg.size() >= 1 + 8);
  REQUIRE(static_cast<uint8_t>(bytes[0]) == 2);  // Time opcode
  REQUIRE(readUint64LE(bytes, 1) == timestamp);
}

TEST_CASE("Broadcast time") {
  auto context = foxglove::Context::create();
  auto server = startServer(context, foxglove::WebSocketServerCapabilities::Time);

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  server.broadcastTime(42);

  // Wait for the time message.
  payload = client.recv();
  validateTimeMessage(payload, 42);

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("Clear session") {
  auto context = foxglove::Context::create();
  auto server = startServer(context);

  // Set an initial session ID.
  auto error = server.clearSession("initial");
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");
  REQUIRE(parsed.contains("sessionId"));
  std::string session_id1 = parsed["sessionId"].get<std::string>();
  REQUIRE(session_id1 == "initial");

  // Reset the session without specifying a new session ID.
  error = server.clearSession();
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Wait for the serverInfo message.
  payload = client.recv();
  parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");
  REQUIRE(parsed.contains("sessionId"));
  std::string session_id2 = parsed["sessionId"].get<std::string>();
  REQUIRE(session_id1 != session_id2);

  // Reset the session with an explicit session ID.
  error = server.clearSession("foo");
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Wait for the serverInfo message.
  payload = client.recv();
  parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");
  REQUIRE(parsed.contains("sessionId"));
  std::string session_id3 = parsed["sessionId"].get<std::string>();
  REQUIRE(session_id3 == "foo");

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("Publish status") {
  auto context = foxglove::Context::create();
  auto server = startServer(context);

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  // Publish status without an ID.
  auto error = server.publishStatus(foxglove::WebSocketServerStatusLevel::Info, "hooray");
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Wait for status message.
  payload = client.recv();
  parsed = Json::parse(payload);
  auto expected = Json::parse(R"({
      "op": "status",
      "level": 0,
      "message": "hooray"
    })");
  REQUIRE(parsed == expected);

  // Publish status with an ID.
  error = server.publishStatus(foxglove::WebSocketServerStatusLevel::Warning, "oh no", "id1");
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Wait for status message.
  payload = client.recv();
  parsed = Json::parse(payload);
  expected = Json::parse(R"({
      "op": "status",
      "level": 1,
      "message": "oh no",
      "id": "id1"
    })");
  REQUIRE(parsed == expected);

  // Remove status messages by ID.
  error = server.removeStatus({"id1", "id2"});
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Wait for removeStatus message.
  payload = client.recv();
  parsed = Json::parse(payload);
  expected = Json::parse(R"({
      "op": "removeStatus",
      "statusIds": ["id1", "id2"]
    })");
  REQUIRE(parsed == expected);

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("Log message to websocket sinks") {
  std::mutex mutex;
  std::condition_variable cv;

  auto context = foxglove::Context::create();

  std::vector<uint64_t> client_sink_ids;
  auto channel_result = foxglove::RawChannel::create("test", "json", std::nullopt, context);
  REQUIRE(channel_result.has_value());

  foxglove::RawChannel channel = std::move(channel_result.value());

  foxglove::WebSocketServerCallbacks cb;
  cb.onSubscribe = [&](uint64_t subscribed_channel_id, const foxglove::ClientMetadata& metadata) {
    std::scoped_lock lock{mutex};
    if (subscribed_channel_id == channel.id() && metadata.sink_id.has_value()) {
      client_sink_ids.push_back(metadata.sink_id.value());
    }
    cv.notify_one();
  };

  auto server = startServer(context, foxglove::WebSocketServerCapabilities(0), std::move(cb));

  // Set up a few clients and connect them
  constexpr size_t num_clients = 3;
  std::vector<std::unique_ptr<WebSocketClient>> clients;
  for (size_t i = 0; i < num_clients; ++i) {
    clients.emplace_back(std::make_unique<WebSocketClient>());
    clients.back()->start(server.port());
    clients.back()->waitForConnection();
  }

  // Flush the serverInfo and advertise messages from each client
  for (auto& client : clients) {
    auto server_info = client->filterRecv([](const std::string& payload) {
      auto parsed = Json::parse(payload);
      return parsed.contains("op") && parsed["op"] == "serverInfo";
    });
    REQUIRE(server_info.has_value());

    auto advertise_response = client->filterRecv([](const std::string& payload) {
      auto parsed = Json::parse(payload);
      return parsed.contains("op") && parsed["op"] == "advertise";
    });
    REQUIRE(advertise_response.has_value());
  }

  // Subscribe clients to the channel
  uint64_t subscription_id = 100;
  for (auto& client : clients) {
    client->send(
      R"({
      "op": "subscribe",
      "subscriptions": [{"id": )" +
      std::to_string(subscription_id) + R"(, "channelId": )" + std::to_string(channel.id()) +
      R"(}]})"
    );
    subscription_id++;
  }

  // Wait for subscriptions to set up
  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1), [&] {
      return client_sink_ids.size() == num_clients;
    });
    REQUIRE(wait_result);
  }

  uint64_t clients_received_message = 0;
  std::string message = R"({"data": "foxglove"})";
  auto messageDataPredicate = [](const std::string& payload) {
    // Only count messages that start with opcode 1 (MessageData)
    return payload[0] == '\x01';
  };

  SECTION("Log message to a specific sink") {
    uint64_t target_sink_id = client_sink_ids[0];

    // Log message to channel but target only a single client
    channel.log(
      reinterpret_cast<const std::byte*>(message.data()),
      message.size(),
      std::nullopt,
      target_sink_id
    );

    for (auto& client : clients) {
      auto message_response = client->filterRecv(messageDataPredicate);

      if (message_response.has_value()) {
        ++clients_received_message;
      }
    }

    REQUIRE(clients_received_message == 1);
  }

  SECTION("Log message to all sinks") {
    // Log message to channel and target all sinks
    channel.log(reinterpret_cast<const std::byte*>(message.data()), message.size());

    for (auto& client : clients) {
      auto message_response = client->filterRecv(messageDataPredicate);

      if (message_response.has_value()) {
        ++clients_received_message;
      }
    }

    REQUIRE(clients_received_message == num_clients);
  }
}

TEST_CASE("Server channel filtering") {
  auto context = foxglove::Context::create();
  std::mutex mutex;
  std::condition_variable cv;
  // the following variable is protected by the mutex:
  std::vector<uint64_t> subscribe_calls;
  std::unique_lock lock{mutex};

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onSubscribe =
    [&](uint64_t channel_id, const foxglove::ClientMetadata& _ [[maybe_unused]]) {
      std::scoped_lock lock{mutex};
      std::cerr << "onSubscribe: " << channel_id << std::endl;
      subscribe_calls.push_back(channel_id);
      cv.notify_all();
    };

  foxglove::WebSocketServerOptions ws_options;
  ws_options.context = context;
  ws_options.callbacks = std::move(callbacks);
  ws_options.sink_channel_filter = [](foxglove::ChannelDescriptor&& channel) -> bool {
    return channel.topic() == "/1";
  };

  auto server = startServer(std::move(ws_options));

  auto channel_result_1 = foxglove::RawChannel::create("/1", "json", std::nullopt, context);
  REQUIRE(channel_result_1.has_value());
  auto channel_1 = std::move(channel_result_1.value());

  auto channel_result_2 = foxglove::RawChannel::create("/2", "json", std::nullopt, context);
  REQUIRE(channel_result_2.has_value());
  auto channel_2 = std::move(channel_result_2.value());

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  payload = client.recv();
  std::cerr << "payload: " << payload << std::endl;
  parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "advertise");
  REQUIRE(parsed["channels"].size() == 1);
  REQUIRE(parsed["channels"][0]["id"] == channel_1.id());

  client.send(
    R"({
      "op": "subscribe",
      "subscriptions": [
        {
          "id": 100, "channelId": )" +
    std::to_string(channel_1.id()) + R"( }
      ]
    })"
  );

  // Channel 2 is filtered, unadvertised, and can't be subscribed to.
  client.send(
    R"({
      "op": "subscribe",
      "subscriptions": [
        {
          "id": 101, "channelId": )" +
    std::to_string(channel_2.id()) + R"( }
      ]
    })"
  );

  cv.wait_for(lock, std::chrono::seconds(1), [&] {
    return !subscribe_calls.empty();
  });
  REQUIRE_THAT(subscribe_calls, Equals(std::vector<uint64_t>{1}));

  payload = client.recv();
  parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "status");
  REQUIRE(parsed["message"] == "Unknown channel ID: " + std::to_string(channel_2.id()));

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("Server info") {
  auto context = foxglove::Context::create();

  std::map<std::string, std::string> server_info = {{"key1", "value1"}};

  foxglove::WebSocketServerOptions ws_options;
  ws_options.context = context;
  ws_options.server_info = server_info;

  auto server = startServer(std::move(ws_options));

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");
  auto metadata = parsed["metadata"];
  auto iterator = metadata.find("key1");
  REQUIRE(iterator != metadata.end());
  REQUIRE(*iterator == "value1");

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

std::vector<std::byte> playerStateToBinary(const foxglove::PlayerState& player_state) {
  constexpr size_t MESSAGE_SIZE = 14;
  std::vector<std::byte> msg;
  msg.reserve(MESSAGE_SIZE);

  msg.push_back(std::byte{0x03});
  msg.emplace_back(std::byte{
    static_cast<std::underlying_type_t<foxglove::PlaybackState>>(player_state.playback_state)
  });

  writeFloatLE(msg, player_state.playback_speed);

  msg.emplace_back(player_state.seek_time.has_value() ? std::byte{0x1} : std::byte{0x0});
  writeIntLE(msg, player_state.seek_time.has_value() ? *player_state.seek_time : 0x0);

  return msg;
}

TEST_CASE("Player state callback") {
  auto context = foxglove::Context::create();

  std::optional<foxglove::PlayerState> received_player_state = std::nullopt;
  std::mutex mutex;
  std::condition_variable cv;

  foxglove::WebSocketServerCallbacks callbacks;
  callbacks.onPlayerState = [&]([[maybe_unused]] const foxglove::PlayerState& player_state) {
    {
      std::unique_lock lock(mutex);
      received_player_state = std::make_optional<foxglove::PlayerState>(player_state);
    }
    cv.notify_one();
  };

  auto server = startServer(
    context, foxglove::WebSocketServerCapabilities::RangedPlayback, std::move(callbacks)
  );

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  foxglove::PlayerState player_state{
    .playback_state = foxglove::PlaybackState::Paused,
    .playback_speed = 1.0,
    .seek_time = 42,
  };
  std::vector<std::byte> msg = playerStateToBinary(player_state);
  client.send(msg);

  {
    std::unique_lock lock{mutex};
    auto wait_result = cv.wait_for(lock, std::chrono::seconds(1));
    REQUIRE(wait_result != std::cv_status::timeout);
    REQUIRE(received_player_state.has_value());
    REQUIRE(received_player_state->playback_state == foxglove::PlaybackState::Paused);
    REQUIRE(received_player_state->playback_speed == 1.0);
    REQUIRE(received_player_state->seek_time.has_value());
    REQUIRE(received_player_state->seek_time.value() == 42);
  }

  REQUIRE(server.stop() == foxglove::FoxgloveError::Ok);
}

TEST_CASE("RangedPlayback capability") {
  auto context = foxglove::Context::create();

  const uint64_t start_time = 100e9;
  const uint64_t end_time = 105e9;

  foxglove::WebSocketServerOptions opt;
  opt.context = std::move(context);
  opt.playback_time_range = std::make_optional<std::pair<uint64_t, uint64_t>>(start_time, end_time);
  auto server = startServer(std::move(opt));

  WebSocketClient client;
  client.start(server.port());
  client.waitForConnection();

  auto payload = client.recv();
  auto parsed = Json::parse(payload);
  REQUIRE(parsed.contains("op"));
  REQUIRE(parsed["op"] == "serverInfo");

  // Ensure that the rangedPlayback capability is enabled, since opt.playback_time_range is
  // specified
  REQUIRE(parsed.contains("capabilities"));
  const auto& capabilities = parsed["capabilities"];
  // TODO: Left off here: Why is the capability being added twice??
  REQUIRE(std::count_if(capabilities.begin(), capabilities.end(), [](const auto& capability) {
            return capability == "rangedPlayback";
          }) == 1);

  REQUIRE(parsed.contains("dataStartTime"));
  REQUIRE(parsed["dataStartTime"] == start_time);
  REQUIRE(parsed.contains("dataEndTime"));
  REQUIRE(parsed["dataEndTime"] == end_time);
}
