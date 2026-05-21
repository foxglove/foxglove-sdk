/**
 * Foxglove Parameter Server
 *
 * An example from the Foxglove SDK.
 *
 * This implements a parameter server using the `ParameterHandler` API.
 * Get/set requests from clients are enqueued on a worker thread, which owns
 * the parameter store and fulfils each responder. Because
 * `SetParametersResponder` only echoes the applied values to the requester,
 * the worker is also responsible for publishing those updates to other
 * parameter subscribers; the same path is used to publish a periodic
 * "elapsed" tick. The parameter store has exactly one owner, so no
 * synchronization is required.
 *
 * When built with remote-access support (`FOXGLOVE_REMOTE_ACCESS`) and the
 * `FOXGLOVE_DEVICE_TOKEN` environment variable is set, the example also
 * starts a remote-access gateway that shares the same parameter handler, so
 * the parameter store is reachable from both WebSocket clients and remote
 * Foxglove sessions.
 *
 * View and edit parameters from a Parameters panel in Foxglove:
 * https://docs.foxglove.dev/docs/visualization/panels/parameters
 */

#include <foxglove/foxglove.hpp>
#include <foxglove/parameter.hpp>
#include <foxglove/parameter_handler.hpp>
#include <foxglove/websocket.hpp>

#ifdef FOXGLOVE_REMOTE_ACCESS
#include <foxglove/remote_access.hpp>
#endif

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <csignal>
#include <cstdlib>
#include <functional>
#include <iostream>
#include <mutex>
#include <optional>
#include <queue>
#include <string>
#include <thread>
#include <unordered_map>
#include <utility>
#include <variant>
#include <vector>

using namespace std::chrono_literals;

// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
static std::function<void()> sigint_handler;

namespace {

struct GetOp {
  std::vector<std::string> names;
  foxglove::GetParametersResponder responder;
};

struct SetOp {
  std::vector<foxglove::Parameter> parameters;
  foxglove::SetParametersResponder responder;
};

using ParameterOp = std::variant<GetOp, SetOp>;

/// @brief Thread-safe queue of parameter operations enqueued by the SDK and
/// drained by the worker thread.
class OpQueue {
public:
  void push(ParameterOp&& op) {
    {
      std::lock_guard<std::mutex> lock(mu_);
      queue_.push(std::move(op));
    }
    cv_.notify_one();
  }

  void shutdown() {
    {
      std::lock_guard<std::mutex> lock(mu_);
      shutdown_ = true;
    }
    cv_.notify_all();
  }

  /// Block until an op is available, or shutdown is signalled. Returns
  /// `std::nullopt` if the queue is shut down and empty, or if the timeout
  /// expires.
  std::optional<ParameterOp> pop(std::chrono::milliseconds timeout) {
    std::unique_lock<std::mutex> lock(mu_);
    cv_.wait_for(lock, timeout, [&] {
      return shutdown_ || !queue_.empty();
    });
    if (queue_.empty()) {
      return std::nullopt;
    }
    auto op = std::move(queue_.front());
    queue_.pop();
    return op;
  }

private:
  std::mutex mu_;
  std::condition_variable cv_;
  std::queue<ParameterOp> queue_;
  bool shutdown_ = false;
};

}  // namespace

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  std::signal(SIGINT, [](int) {
    if (sigint_handler) {
      sigint_handler();
    }
  });

  std::unordered_map<std::string, foxglove::Parameter> param_store;
  param_store.emplace(
    "read_only_str", foxglove::Parameter("read_only_str", std::string("can't change me"))
  );
  param_store.emplace("elapsed", foxglove::Parameter("elapsed", 0.0));
  param_store.emplace(
    "float_array", foxglove::Parameter("float_array", std::vector<double>{1.0, 2.0, 3.0})
  );

  OpQueue queue;

  // Build a parameter handler that just enqueues each request onto the worker
  // queue. The same handler is shared between the WebSocket server and (when
  // available) the remote-access gateway.
  foxglove::ParameterHandler handler;
  handler.onGet = [&queue](
                    uint32_t /*client_id*/,
                    std::optional<std::string_view> /*request_id*/,
                    const std::vector<std::string_view>& param_names,
                    foxglove::GetParametersResponder&& responder
                  ) {
    GetOp op{{}, std::move(responder)};
    op.names.reserve(param_names.size());
    for (const auto& name : param_names) {
      op.names.emplace_back(name);
    }
    queue.push(std::move(op));
  };
  handler.onSet = [&queue](
                    uint32_t /*client_id*/,
                    std::optional<std::string_view> /*request_id*/,
                    const std::vector<foxglove::ParameterView>& params,
                    foxglove::SetParametersResponder&& responder
                  ) {
    SetOp op{{}, std::move(responder)};
    op.parameters.reserve(params.size());
    for (const auto& param : params) {
      op.parameters.emplace_back(param.clone());
    }
    queue.push(std::move(op));
  };

  foxglove::WebSocketServerOptions options = {};
  options.name = "param-server";
  options.host = "127.0.0.1";
  options.port = 8765;
  // Registering a ParameterHandler implicitly enables the Parameters capability.
  options.parameter_handler = handler;

  auto server_result = foxglove::WebSocketServer::create(std::move(options));
  if (!server_result.has_value()) {
    std::cerr << "Failed to create server: " << foxglove::strerror(server_result.error()) << '\n';
    return 1;
  }
  auto server = std::move(server_result.value());

#ifdef FOXGLOVE_REMOTE_ACCESS
  // Optionally start a remote-access gateway when FOXGLOVE_DEVICE_TOKEN is set.
  std::optional<foxglove::RemoteAccessGateway> gateway;
#if defined(_MSC_VER)
#pragma warning(push)
#pragma warning(disable : 4996)  // 'getenv': MSVC deprecation; single-threaded example startup.
#endif
  // NOLINTNEXTLINE(concurrency-mt-unsafe): single-threaded example startup.
  const bool have_device_token = std::getenv("FOXGLOVE_DEVICE_TOKEN") != nullptr;
#if defined(_MSC_VER)
#pragma warning(pop)
#endif
  if (have_device_token) {
    foxglove::RemoteAccessGatewayOptions ra_options = {};
    ra_options.name = "param-server";
    ra_options.parameter_handler = handler;
    auto gateway_result = foxglove::RemoteAccessGateway::create(std::move(ra_options));
    if (!gateway_result.has_value()) {
      std::cerr << "Failed to start remote-access gateway: "
                << foxglove::strerror(gateway_result.error()) << '\n';
      return 1;
    }
    gateway.emplace(std::move(gateway_result.value()));
  }
#endif

  std::atomic_bool done = false;
  sigint_handler = [&] {
    done = true;
    queue.shutdown();
  };

  // Publishes parameter values to all subscribers across the server and (if
  // configured) the remote-access gateway.
  auto publish = [&](std::vector<foxglove::Parameter> params) {
#ifdef FOXGLOVE_REMOTE_ACCESS
    if (gateway) {
      std::vector<foxglove::Parameter> cloned;
      cloned.reserve(params.size());
      for (const auto& p : params) {
        cloned.push_back(p.clone());
      }
      gateway->publishParameterValues(std::move(cloned));
    }
#endif
    server.publishParameterValues(std::move(params));
  };

  auto start_time = std::chrono::steady_clock::now();
  auto next_tick = start_time + 1s;
  while (!done) {
    auto now = std::chrono::steady_clock::now();
    auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(next_tick - now);
    if (remaining.count() < 0) {
      remaining = 0ms;
    }
    auto maybe_op = queue.pop(remaining);
    if (maybe_op) {
      std::visit(
        [&](auto& op) {
          using T = std::decay_t<decltype(op)>;
          if constexpr (std::is_same_v<T, GetOp>) {
            std::vector<foxglove::Parameter> result;
            if (op.names.empty()) {
              result.reserve(param_store.size());
              for (const auto& it : param_store) {
                result.push_back(it.second.clone());
              }
            } else {
              for (const auto& name : op.names) {
                if (auto it = param_store.find(name); it != param_store.end()) {
                  result.push_back(it->second.clone());
                }
              }
            }
            std::move(op.responder).respond(std::move(result));
          } else if constexpr (std::is_same_v<T, SetOp>) {
            std::vector<foxglove::Parameter> result;
            std::vector<foxglove::Parameter> applied;
            for (auto& param : op.parameters) {
              const std::string name(param.name());
              auto it = param_store.find(name);
              if (it != param_store.end()) {
                if (name.rfind("read_only_", 0) == 0) {
                  // Echo back the existing value so the client sees no change.
                  result.push_back(it->second.clone());
                  continue;
                }
                it->second = std::move(param);
              } else {
                it = param_store.emplace(name, std::move(param)).first;
              }
              // The store now owns the value. Clone twice: `applied` is
              // broadcast to subscribers, `result` is echoed to the requester.
              applied.push_back(it->second.clone());
              result.push_back(it->second.clone());
            }
            std::move(op.responder).respond(std::move(result));
            // SetParametersResponder only echoes to the requester, so publish
            // applied changes to subscribers ourselves.
            if (!applied.empty()) {
              publish(std::move(applied));
            }
          }
        },
        *maybe_op
      );
    }

    if (std::chrono::steady_clock::now() >= next_tick) {
      auto elapsed_secs =
        std::chrono::duration<double>(std::chrono::steady_clock::now() - start_time).count();
      auto elapsed = foxglove::Parameter("elapsed", elapsed_secs);
      param_store.insert_or_assign("elapsed", elapsed.clone());
      std::vector<foxglove::Parameter> to_publish;
      to_publish.emplace_back(std::move(elapsed));
      publish(std::move(to_publish));
      next_tick += 1s;
    }
  }

#ifdef FOXGLOVE_REMOTE_ACCESS
  if (gateway) {
    gateway->stop();
  }
#endif
  server.stop();
  return 0;
}
