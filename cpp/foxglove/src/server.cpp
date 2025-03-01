#include <foxglove-c/foxglove-c.h>
#include <foxglove/server.hpp>

namespace foxglove {

WebSocketServer::WebSocketServer(WebSocketServerOptions options)
    : _impl(
        foxglove_server_start(options.name.c_str(), options.host.c_str(), options.port),
        foxglove_server_free
      ) {}

void WebSocketServer::stop() {
  foxglove_server_stop(_impl.get());
}

uint16_t WebSocketServer::port() const {
  return foxglove_server_get_port(_impl.get());
}

}  // namespace foxglove
