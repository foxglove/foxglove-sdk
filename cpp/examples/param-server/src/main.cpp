/**
 * Foxglove Parameter Server
 *
 * An example from the Foxglove SDK.
 *
 * This implements a parameter server for live visualization.
 *
 * View and edit parameters from a Parameters panel in Foxglove:
 * https://docs.foxglove.dev/docs/visualization/panels/parameters
 */

#include <foxglove/foxglove.hpp>
#include <foxglove/server.hpp>
#include <foxglove/server/parameter.hpp>

#include <atomic>
#include <chrono>
#include <csignal>
#include <functional>
#include <iostream>
#include <thread>
#include <unordered_map>

using namespace std::chrono_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  // Initialize parameter store
  std::vector<foxglove::Parameter> params;
  params.emplace_back("read_only_str", std::string("can't change me"));
  params.emplace_back("elapsed", 1.0);
  params.emplace_back("float_array", std::vector<double>{1.0, 2.0, 3.0});

  std::unordered_map<std::string, foxglove::Parameter> param_store;
  for (auto&& param : std::move(params)) {
    param_store.emplace(param.name(), std::move(param));
  }

  foxglove::WebSocketServerOptions options = {};
  options.name = "param-server";
  options.host = "127.0.0.1";
  options.port = 8765;
  options.capabilities = foxglove::WebSocketServerCapabilities::Parameters;
  options.callbacks.onGetParameters = [&param_store](
                                        uint32_t client_id [[maybe_unused]],
                                        std::optional<std::string_view>
                                          request_id,
                                        const std::vector<std::string_view>& param_names
                                      ) -> std::vector<foxglove::Parameter> {
    std::vector<foxglove::Parameter> result;
    std::cerr << "onGetParameters called";
    if (request_id.has_value()) {
      std::cerr << " with request_id '" << *request_id << "'";
    }
    if (param_names.empty()) {
      std::cerr << " for all parameters\n";
      for (const auto& it : param_store) {
        result.push_back(it.second.clone());
      }
    } else {
      std::cerr << " for parameters:\n";
      for (const auto& name : param_names) {
        std::cerr << " - " << name << "\n";
        if (auto it = param_store.find(std::string(name)); it != param_store.end()) {
          result.push_back(it->second.clone());
        }
      }
    }
    return result;
  };
  options.callbacks.onSetParameters = [&param_store](
                                        uint32_t client_id [[maybe_unused]],
                                        std::optional<std::string_view>
                                          request_id,
                                        const std::vector<foxglove::ParameterView>& params
                                      ) -> std::vector<foxglove::Parameter> {
    std::cerr << "onSetParameters called";
    if (request_id.has_value()) {
      std::cerr << " with request_id '" << *request_id << "'";
    }
    std::cerr << " for parameters:\n";
    std::vector<foxglove::Parameter> result;
    for (const auto& param : params) {
      std::cerr << " - " << param.name();
      const std::string name(param.name());
      if (auto it = param_store.find(name); it != param_store.end()) {
        if (name.find("read_only_") == 0) {
          std::cerr << " - not updated\n";
          result.emplace_back(it->second.clone());
        } else {
          std::cerr << " - updated\n";
          it->second = param.clone();
          result.emplace_back(param.clone());
        }
      }
    }
    return result;
  };
  options.callbacks.onParametersSubscribe = [](const std::vector<std::string_view>& names) {
    std::cerr << "onParametersSubscribe called for parameters:\n";
    for (const auto& name : names) {
      std::cerr << " - " << name << "\n";
    }
  };
  options.callbacks.onParametersUnsubscribe = [](const std::vector<std::string_view>& names) {
    std::cerr << "onParametersUnsubscribe called for parameters:\n";
    for (const auto& name : names) {
      std::cerr << " - " << name << "\n";
    }
  };

  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error()) << '\n';
    return 1;
  }
  auto server = std::move(server_result.value());

  std::atomic_bool done = false;
  sigint_handler = [&] {
    done = true;
  };

  // Start timer
  auto start_time = std::chrono::steady_clock::now();
  while (!done) {
    std::this_thread::sleep_for(100ms);
    // Update elapsed time
    auto now = std::chrono::steady_clock::now();
    auto elapsed = std::chrono::duration<double>(now - start_time).count();
    auto param = foxglove::Parameter("elapsed", elapsed);
    param_store.insert_or_assign("elapsed", param.clone());
    std::vector<foxglove::Parameter> paramsToPublish;
    paramsToPublish.emplace_back(std::move(param));
    server.publishParameterValues(std::move(paramsToPublish));
  }

  server.stop();
  return 0;
}
