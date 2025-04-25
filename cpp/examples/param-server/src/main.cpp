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
static std::function<void()> sigintHandler;

// NOLINTNEXTLINE(bugprone-exception-escape)
int main(int argc, const char* argv[]) {
  std::signal(SIGINT, [](int) {
    if (sigintHandler) {
      sigintHandler();
    }
  });

  // Initialize parameter store
  std::vector<foxglove::Parameter> params;
  params.emplace_back("read_only_str", std::string("can't change me"));
  params.emplace_back("elapsed", 1.0);
  params.emplace_back("float_array", std::vector<double>{1.0, 2.0, 3.0});

  std::unordered_map<std::string, foxglove::Parameter> paramStore;
  for (auto&& param : std::move(params)) {
    paramStore.emplace(param.name(), std::move(param));
  }

  foxglove::WebSocketServerOptions options = {};
  options.name = "param-server";
  options.host = "127.0.0.1";
  options.port = 8765;
  options.capabilities = foxglove::WebSocketServerCapabilities::Parameters;
  options.callbacks.onGetParameters =
    [&paramStore](
      uint32_t clientId, std::string_view request_id, const std::vector<std::string>& param_names
    ) -> std::vector<foxglove::Parameter> {
    std::vector<foxglove::Parameter> result;
    std::cerr << "onGetParameters called with request_id '" << request_id << "'";
    if (param_names.empty()) {
      std::cerr << " for all parameters\n";
      for (const auto& it : paramStore) {
        result.push_back(it.second.clone());
      }
    } else {
      std::cerr << " for parameters:\n";
      for (const auto& name : param_names) {
        std::cerr << " - " << name << "\n";
        if (auto it = paramStore.find(name); it != paramStore.end()) {
          result.push_back(it->second.clone());
        }
      }
    }
    return result;
  };
  options.callbacks.onSetParameters = [&paramStore](
                                        uint32_t clientId,
                                        std::string_view request_id,
                                        const std::vector<foxglove::ParameterView>& params
                                      ) -> std::vector<foxglove::Parameter> {
    std::cerr << "onSetParameters called with request_id '" << request_id << "' for parameters:\n";
    std::vector<foxglove::Parameter> result;
    for (const auto& param : params) {
      std::cerr << " - " << param.name();
      const std::string name = param.name();
      if (auto it = paramStore.find(name); it != paramStore.end()) {
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

  auto serverResult = foxglove::WebSocketServer::create(std::move(options));
  if (!serverResult.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(serverResult.error()) << '\n';
    return 1;
  }
  auto server = std::move(serverResult.value());
  std::cerr << "Started server\n";

  std::atomic_bool done = false;
  sigintHandler = [&] {
    std::cerr << "Shutting down...\n";
    server.stop();
    done = true;
  };

  // Start timer
  auto startTime = std::chrono::steady_clock::now();
  while (!done) {
    std::this_thread::sleep_for(100ms);
    // Update elapsed time
    auto now = std::chrono::steady_clock::now();
    auto elapsed = std::chrono::duration<double>(now - startTime).count();
    paramStore.insert_or_assign("elapsed", foxglove::Parameter("elapsed", elapsed));
  }

  std::cerr << "Done\n";
  return 0;
}
