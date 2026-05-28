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
 * View and edit parameters from a Parameters panel in Foxglove:
 * https://docs.foxglove.dev/docs/visualization/panels/parameters
 */

#include <foxglove/foxglove.hpp>
#include <foxglove/parameter.hpp>
#include <foxglove/parameter_handler.hpp>
#include <foxglove/websocket.hpp>

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <csignal>
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

namespace {

// Set by the SIGINT handler.
// NOLINTNEXTLINE(cppcoreguidelines-avoid-non-const-global-variables)
std::atomic<bool> g_shutdown{false};
static_assert(decltype(g_shutdown)::is_always_lock_free);

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

  /// Block until an op is available or the timeout expires.
  std::optional<ParameterOp> pop(std::chrono::milliseconds timeout) {
    std::unique_lock<std::mutex> lock(mu_);
    cv_.wait_for(lock, timeout, [&] {
      return !queue_.empty();
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
};

}  // namespace

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  foxglove::setLogLevel(foxglove::LogLevel::Info);

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
  // queue.
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

  std::signal(SIGINT, [](int) {
    g_shutdown.store(true, std::memory_order_relaxed);
  });

  auto start_time = std::chrono::steady_clock::now();
  auto next_tick = start_time + 1s;
  while (!g_shutdown.load(std::memory_order_relaxed)) {
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
              server.publishParameterValues(std::move(applied));
            }
          }
        },
        *maybe_op
      );
    }

    now = std::chrono::steady_clock::now();
    if (now >= next_tick) {
      auto elapsed_secs = std::chrono::duration<double>(now - start_time).count();
      auto elapsed = foxglove::Parameter("elapsed", elapsed_secs);
      param_store.insert_or_assign("elapsed", elapsed.clone());
      std::vector<foxglove::Parameter> to_publish;
      to_publish.emplace_back(std::move(elapsed));
      server.publishParameterValues(std::move(to_publish));
      next_tick += 1s;
    }
  }

  server.stop();
  return 0;
}
