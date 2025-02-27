#include <cstdint>
#include <memory>
#include <string>

struct foxglove_websocket_server;

namespace foxglove {

struct WebSocketServerOptions {
  std::string name;
  std::string host;
  uint16_t port;
};

class WebSocketServer {
public:
  WebSocketServer(WebSocketServerOptions options);
  void stop();

private:
  std::unique_ptr<foxglove_websocket_server, void (*)(foxglove_websocket_server*)> _impl;
};

}  // namespace foxglove
