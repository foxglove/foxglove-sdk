#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>
#include <foxglove/server/connection_graph.hpp>

#include <type_traits>

namespace foxglove {

FoxgloveResult<WebSocketServer> WebSocketServer::create(
  WebSocketServerOptions&& options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  bool hasAnyCallbacks = options.callbacks.onSubscribe || options.callbacks.onUnsubscribe ||
                         options.callbacks.onClientAdvertise || options.callbacks.onMessageData ||
                         options.callbacks.onClientUnadvertise ||
                         options.callbacks.onGetParameters || options.callbacks.onSetParameters ||
                         options.callbacks.onConnectionGraphSubscribe ||
                         options.callbacks.onConnectionGraphUnsubscribe;

  std::unique_ptr<WebSocketServerCallbacks> callbacks;

  foxglove_server_callbacks cCallbacks = {};

  if (hasAnyCallbacks) {
    callbacks = std::make_unique<WebSocketServerCallbacks>(std::move(options.callbacks));
    cCallbacks.context = callbacks.get();
    if (callbacks->onSubscribe) {
      cCallbacks.on_subscribe = [](uint64_t channel_id, const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onSubscribe(channel_id);
      };
    }
    if (callbacks->onUnsubscribe) {
      cCallbacks.on_unsubscribe = [](uint64_t channel_id, const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onUnsubscribe(channel_id);
      };
    }
    if (callbacks->onClientAdvertise) {
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
          (static_cast<const WebSocketServerCallbacks*>(context))
            ->onClientAdvertise(client_id, cppChannel);
        };
    }
    if (callbacks->onMessageData) {
      cCallbacks.on_message_data = [](
                                     // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
                                     uint32_t client_id,
                                     uint32_t client_channel_id,
                                     const uint8_t* payload,
                                     size_t payload_len,
                                     const void* context
                                   ) {
        (static_cast<const WebSocketServerCallbacks*>(context))
          ->onMessageData(
            client_id, client_channel_id, reinterpret_cast<const std::byte*>(payload), payload_len
          );
      };
    }
    if (callbacks->onClientUnadvertise) {
      cCallbacks.on_client_unadvertise =
        // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
        [](uint32_t client_id, uint32_t client_channel_id, const void* context) {
          (static_cast<const WebSocketServerCallbacks*>(context))
            ->onClientUnadvertise(client_id, client_channel_id);
        };
    }
    if (callbacks->onGetParameters) {
      cCallbacks.on_get_parameters = [](
                                       const void* context,
                                       uint32_t client_id,
                                       const char* _request_id,
                                       const char* const* _param_names,
                                       size_t param_names_len
                                     ) -> foxglove_parameter_array* {
        std::string request_id(_request_id);
        std::vector<std::string> param_names;
        param_names.reserve(param_names_len);
        for (auto i = 0; i < param_names_len; ++i) {
          param_names.emplace_back(_param_names[i]);
        }
        auto params = (static_cast<const WebSocketServerCallbacks*>(context))
                        ->onGetParameters(client_id, request_id, param_names);
        auto array = ParameterArray(std::move(params));
        return array.release();
      };
    }
    if (callbacks->onSetParameters) {
      cCallbacks.on_set_parameters = [](
                                       const void* context,
                                       uint32_t client_id,
                                       const char* _request_id,
                                       const foxglove_parameter_array* _params
                                     ) -> foxglove_parameter_array* {
        std::string request_id(_request_id);
        auto params =
          (static_cast<const WebSocketServerCallbacks*>(context))
            ->onSetParameters(client_id, request_id, ParameterArrayView(_params).parameters());
        auto array = ParameterArray(std::move(params));
        return array.release();
      };
    }
    if (callbacks->onConnectionGraphSubscribe) {
      cCallbacks.on_connection_graph_subscribe = [](const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onConnectionGraphSubscribe();
      };
    }
    if (callbacks->onConnectionGraphUnsubscribe) {
      cCallbacks.on_connection_graph_unsubscribe = [](const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onConnectionGraphUnsubscribe();
      };
    }
  }

  foxglove_server_options cOptions = {};
  cOptions.name = {options.name.c_str(), options.name.length()};
  cOptions.host = {options.host.c_str(), options.host.length()};
  cOptions.port = options.port;
  cOptions.callbacks = hasAnyCallbacks ? &cCallbacks : nullptr;
  cOptions.capabilities =
    static_cast<std::underlying_type_t<decltype(options.capabilities)>>(options.capabilities);
  std::vector<foxglove_string> supportedEncodings;
  supportedEncodings.reserve(options.supportedEncodings.size());
  for (const auto& encoding : options.supportedEncodings) {
    supportedEncodings.push_back({encoding.c_str(), encoding.length()});
  }
  cOptions.supported_encodings = supportedEncodings.data();
  cOptions.supported_encodings_count = supportedEncodings.size();

  foxglove_websocket_server* server = nullptr;
  foxglove_error error = foxglove_server_start(&cOptions, &server);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || server == nullptr) {
    return foxglove::unexpected(static_cast<FoxgloveError>(error));
  }

  return WebSocketServer(server, std::move(callbacks));
}

WebSocketServer::WebSocketServer(
  foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks
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

void WebSocketServer::publishConnectionGraph(ConnectionGraph& graph) {
  foxglove_server_publish_connection_graph(_impl.get(), graph._impl.get());
}

}  // namespace foxglove
