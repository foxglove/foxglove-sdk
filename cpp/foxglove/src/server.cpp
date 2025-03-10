#include <foxglove-c/foxglove-c.h>
#include <foxglove/server.hpp>

namespace foxglove {

WebSocketServer::WebSocketServer(WebSocketServerOptions options)
    : _callbacks(options.callbacks)
    , _impl(nullptr, foxglove_server_free) {
  foxglove_server_callbacks cCallbacks = {};
  cCallbacks.context = this;
  bool hasAnyCallbacks = false;
  if (options.callbacks.onSubscribe) {
    hasAnyCallbacks = true;
    cCallbacks.on_subscribe = [](uint64_t channel_id, const void* context) {
      (reinterpret_cast<const WebSocketServer*>(context))->_callbacks.onSubscribe(channel_id);
    };
  }
  if (options.callbacks.onUnsubscribe) {
    hasAnyCallbacks = true;
    cCallbacks.on_unsubscribe = [](uint64_t channel_id, const void* context) {
      (reinterpret_cast<const WebSocketServer*>(context))->_callbacks.onUnsubscribe(channel_id);
    };
  }

  foxglove_server_options cOptions = {};
  cOptions.name = options.name.c_str();
  cOptions.host = options.host.c_str();
  cOptions.port = options.port;
  cOptions.callbacks = hasAnyCallbacks ? &cCallbacks : nullptr;
  _impl.reset(foxglove_server_start(&cOptions));
}

void WebSocketServer::stop() {
  foxglove_server_stop(_impl.get());
}

uint16_t WebSocketServer::port() const {
  return foxglove_server_get_port(_impl.get());
}

}  // namespace foxglove
