#pragma once

#include <cstdint>
#include <functional>
#include <memory>
#include <string>

struct foxglove_websocket_server;

namespace foxglove {

struct WebSocketServerCallbacks {
  std::function<void(uint64_t channel_id)> onSubscribe;
  std::function<void(uint64_t channel_id)> onUnsubscribe;
};

struct WebSocketServerOptions {
  std::string name;
  std::string host;
  uint16_t port;
  WebSocketServerCallbacks callbacks;
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
