#include <foxglove/foxglove.hpp>
#include <foxglove/server.hpp>
#include <foxglove/server/service.hpp>

#include <nlohmann/json.hpp>

#include <array>
#include <atomic>
#include <csignal>
#include <iostream>
#include <thread>

using Json = nlohmann::json;

using namespace std::chrono_literals;
using namespace std::string_literals;
using namespace std::string_view_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

bool registerEmptyService(foxglove::WebSocketServer& server);
bool registerEchoService(foxglove::WebSocketServer& server);
bool registerSleepService(foxglove::WebSocketServer& server);
bool registerIntMathServices(foxglove::WebSocketServer& server);

int main() {
  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  foxglove::WebSocketServerOptions options = {};
  options.name = "ws-services";
  options.host = "127.0.0.1";
  options.port = 8765;
  options.capabilities = foxglove::WebSocketServerCapabilities::Services;
  options.supported_encodings = {"json"};
  auto result = foxglove::WebSocketServer::create(std::move(options));
  if (!result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(result.error()) << "\n";
    return 1;
  }

  auto server = std::move(result.value());

  // Register services.
  bool ok = true;
  ok &= registerEmptyService(server);
  ok &= registerEchoService(server);
  ok &= registerSleepService(server);
  ok &= registerIntMathServices(server);
  if (!ok) {
    std::cerr << "Failed to register all services\n";
    return 1;
  }

  std::atomic_bool done = false;
  sigint_handler = [&] {
    done = true;
  };

  while (!done) {
    std::this_thread::sleep_for(100ms);
  }

  server.stop();
  return 0;
}

std::vector<std::byte> makeBytes(std::string_view sv) {
  const auto* data = reinterpret_cast<const std::byte*>(sv.data());
  return {data, data + sv.size()};
}

/**
 * A service that always responds with an empty json object.
 */
bool registerEmptyService(foxglove::WebSocketServer& server) {
  foxglove::ServiceSchema empty_schema{"/std_srvs/Empty"};
  static foxglove::ServiceHandler empty_handler([](
                                                  const foxglove::ServiceRequest& request
                                                  [[maybe_unused]],
                                                  foxglove::ServiceResponder&& responder
                                                ) {
    std::move(responder).respondOk(makeBytes("{}"));
  });
  auto service = foxglove::Service::create("/empty", empty_schema, empty_handler);
  if (!service.has_value()) {
    std::cerr << "Failed to create /empty service: " << foxglove::strerror(service.error()) << "\n";
    return false;
  }
  auto error = server.addService(std::move(*service));
  if (error != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to add /empty service: " << foxglove::strerror(error) << "\n";
    return false;
  }
  return true;
}

/**
 * A service that echoes its input.
 */
bool registerEchoService(foxglove::WebSocketServer& server) {
  foxglove::ServiceSchema empty_schema{"/std_srvs/Empty"};
  static foxglove::ServiceHandler echo_handler(
    [](const foxglove::ServiceRequest& request, foxglove::ServiceResponder&& responder) {
      std::move(responder).respondOk(request.payload);
    }
  );
  auto service = foxglove::Service::create("/echo", empty_schema, echo_handler);
  if (!service.has_value()) {
    std::cerr << "Failed to create /echo service: " << foxglove::strerror(service.error()) << "\n";
    return false;
  }
  auto error = server.addService(std::move(*service));
  if (error != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to add /echo service: " << foxglove::strerror(error) << "\n";
    return false;
  }
  return true;
}

/**
 * A service that sleeps.
 *
 * Services that need to do more heavy lifting should be handled asynchronously,
 * because the callback is invoked from the websocket client's main poll thread.
 */
bool registerSleepService(foxglove::WebSocketServer& server) {
  foxglove::ServiceSchema empty_schema{"/std_srvs/Empty"};
  static foxglove::ServiceHandler sleep_handler([](
                                                  const foxglove::ServiceRequest& request
                                                  [[maybe_unused]],
                                                  foxglove::ServiceResponder&& responder
                                                ) {
    // Spawn a new thread to handle the response, so that we don't block the
    // websocket client's main poll thread.
    std::thread t([responder = std::move(responder)]() mutable {
      std::this_thread::sleep_for(1s);
      std::move(responder).respondOk(makeBytes(R"({"status": "refreshed"})"));
    });
    t.detach();
  });
  auto service = foxglove::Service::create("/sleep", empty_schema, sleep_handler);
  if (!service.has_value()) {
    std::cerr << "Failed to create /sleep service: " << foxglove::strerror(service.error()) << "\n";
    return false;
  }
  auto error = server.addService(std::move(*service));
  if (error != foxglove::FoxgloveError::Ok) {
    std::cerr << "Failed to add /sleep service: " << foxglove::strerror(error) << "\n";
    return false;
  }
  return true;
}

foxglove::ServiceSchema makeIntMathSchema() {
  static auto request_json_schema = makeBytes(R"({
    "type": "object",
    "properties": {
      "a": { "type": "integer" },
      "b": { "type": "integer" }
    },
    "required": ["a", "b"],
    "additionalProperties": false
  })"sv);
  foxglove::ServiceMessageSchema request{
    "json"s,
    foxglove::Schema{
      "IntMathRequest"s,
      "jsonschema"s,
      request_json_schema.data(),
      request_json_schema.size(),
    }
  };

  static auto response_json_schema = makeBytes(R"({
    "type": "object",
    "properties": {
      "result": { "type": "integer" }
    },
    "required": ["result"],
    "additionalProperties": false
  })"sv);
  foxglove::ServiceMessageSchema response{
    "json"s,
    foxglove::Schema{
      "IntMathResponse"s,
      "jsonschema"s,
      response_json_schema.data(),
      response_json_schema.size(),
    }
  };
  return foxglove::ServiceSchema{
    "/custom_srvs/IntMathOps"s,
    request,
    response,
  };
}

void intMathHandlerImpl(
  const foxglove::ServiceRequest& request, foxglove::ServiceResponder&& responder
) {
  // Shared handlers can use `ServiceRequest.service_name` to distinguish the
  // service endpoint.
  uint32_t a = 0;
  uint32_t b = 0;
  try {
    Json args = Json::parse(request.payloadStr());
    args["a"].get_to(a);
    args["b"].get_to(b);
  } catch (const std::exception e) {
    std::string message(e.what());
    message.insert(0, "JSON error: ");
    std::cerr << message << "\n";
    std::move(responder).respondError(message);
    return;
  }

  Json obj;
  if (request.service_name == "/IntMath/add") {
    obj["result"] = a + b;
  } else if (request.service_name == "/IntMath/sub") {
    obj["result"] = a - b;
  } else if (request.service_name == "/IntMath/mul") {
    obj["result"] = a * b;
  } else {
    std::string message("unexpected service: ");
    message.append(request.service_name);
    std::move(responder).respondError(message);
    return;
  }

  std::string str = obj.dump();
  const auto* const data = reinterpret_cast<const std::byte*>(str.data());
  std::vector bytes(data, data + str.size());
  std::move(responder).respondOk(bytes);
}

/**
 * A service that does some simple math on integers.
 *
 * Note that a single service handler can be shared by multiple services.
 */
bool registerIntMathServices(foxglove::WebSocketServer& server) {
  std::array<std::string_view, 3> int_math_service_names = {
    "/IntMath/add",
    "/IntMath/sub",
    "/IntMath/mul",
  };
  auto int_math_schema = makeIntMathSchema();
  foxglove::ServiceHandler int_math_handler(intMathHandlerImpl);
  for (auto name : int_math_service_names) {
    auto service = foxglove::Service::create(name, int_math_schema, int_math_handler);
    if (!service.has_value()) {
      std::cerr << "Failed to create " << name
                << " service: " << foxglove::strerror(service.error()) << "\n";
      return false;
    }
    auto error = server.addService(std::move(*service));
    if (error != foxglove::FoxgloveError::Ok) {
      std::cerr << "Failed to add " << name << " service: " << foxglove::strerror(error) << "\n";
      return false;
    }
  }
  return true;
}
