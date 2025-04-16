#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>

#include <type_traits>

namespace foxglove {

FoxgloveResult<WebSocketServer> WebSocketServer::create(
  WebSocketServerOptions&& options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_server_callbacks cCallbacks = {};
  cCallbacks.context = options.callbackContext;
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
  if (options.callbacks.onClientAdvertise) {
    hasAnyCallbacks = true;
    cCallbacks.on_client_advertise =
      [](uint32_t client_id, const foxglove_client_channel* channel, const void* context) {
        ClientChannel cppChannel = {
          channel->id,
          channel->topic,
          channel->encoding,
          channel->schema_name,
          channel->schema_encoding == nullptr ? std::string_view{} : channel->schema_encoding,
          reinterpret_cast<const std::byte*>(channel->schema),
          channel->schema_len
        };
        (reinterpret_cast<const WebSocketServer*>(context))
          ->_callbacks.onClientAdvertise(client_id, cppChannel);
      };
  }
  if (options.callbacks.onMessageData) {
    hasAnyCallbacks = true;
    cCallbacks.on_message_data = [](
                                   // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
                                   uint32_t client_id,
                                   uint32_t client_channel_id,
                                   const uint8_t* payload,
                                   size_t payload_len,
                                   const void* context
                                 ) {
      (reinterpret_cast<const WebSocketServer*>(context))
        ->_callbacks.onMessageData(
          client_id, client_channel_id, reinterpret_cast<const std::byte*>(payload), payload_len
        );
    };
  }
  if (options.callbacks.onClientUnadvertise) {
    hasAnyCallbacks = true;
    cCallbacks.on_client_unadvertise =
      // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
      [](uint32_t client_id, uint32_t client_channel_id, const void* context) {
        (reinterpret_cast<const WebSocketServer*>(context))
          ->_callbacks.onClientUnadvertise(client_id, client_channel_id);
      };
  }

  foxglove_server_options cOptions = {};
  cOptions.name = options.name.c_str();
  cOptions.host = options.host.c_str();
  cOptions.port = options.port;
  cOptions.callbacks = hasAnyCallbacks ? &cCallbacks : nullptr;
  cOptions.capabilities =
    static_cast<std::underlying_type_t<decltype(options.capabilities)>>(options.capabilities);
  std::vector<const char*> supportedEncodings;
  supportedEncodings.reserve(options.supportedEncodings.size());
  for (const auto& encoding : options.supportedEncodings) {
    supportedEncodings.push_back(encoding.c_str());
  }
  cOptions.supported_encodings = supportedEncodings.data();
  cOptions.supported_encodings_count = supportedEncodings.size();

  foxglove_websocket_server* server = nullptr;
  foxglove_error error = foxglove_server_start(&cOptions, &server);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || server == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return WebSocketServer(server, std::move(options.callbacks));
}

WebSocketServer::WebSocketServer(
  foxglove_websocket_server* server, WebSocketServerCallbacks&& callbacks
)
    : _impl(server, foxglove_server_stop)
    , _callbacks(std::move(callbacks)) {}

FoxgloveError WebSocketServer::stop() {
  foxglove_error error = foxglove_server_stop(_impl.release());
  return FoxgloveError(error);
}

uint16_t WebSocketServer::port() const {
  return foxglove_server_get_port(_impl.get());
}

}  // namespace foxglove
