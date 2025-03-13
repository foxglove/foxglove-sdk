#pragma once

#include <cstdint>
#include <functional>
#include <memory>
#include <string>

struct foxglove_websocket_server;

namespace foxglove {

enum class WebSocketServerCapabilities : uint8_t {
  /// Allow clients to advertise channels to send data messages to the server.
  ClientPublish = 1 << 0,
  /// Allow clients to subscribe and make connection graph updates
  ConnectionGraph = 1 << 1,
  /// Allow clients to get & set parameters.
  Parameters = 1 << 2,
  /// Inform clients about the latest server time.
  ///
  /// This allows accelerated, slowed, or stepped control over the progress of time. If the
  /// server publishes time data, then timestamps of published messages must originate from the
  /// same time source.
  Time = 1 << 3,
  /// Allow clients to call services.
  Services = 1 << 4,
};

struct WebSocketServerCallbacks {
  std::function<void(uint64_t channel_id)> onSubscribe;
  std::function<void(uint64_t channel_id)> onUnsubscribe;
};

struct WebSocketServerOptions {
  std::string name;
  std::string host;
  uint16_t port;
  WebSocketServerCallbacks callbacks;
  WebSocketServerCapabilities capabilities = WebSocketServerCapabilities(0);
};

class WebSocketServer final {
public:
  explicit WebSocketServer(const WebSocketServerOptions& options);

  // Get the port on which the server is listening.
  uint16_t port() const;

  void stop();

private:
  WebSocketServerCallbacks _callbacks;
  std::unique_ptr<foxglove_websocket_server, void (*)(foxglove_websocket_server*)> _impl;
};

}  // namespace foxglove
