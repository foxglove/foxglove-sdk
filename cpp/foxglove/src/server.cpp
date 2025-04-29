#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>
#include <foxglove/server/connection_graph.hpp>

#include <type_traits>

namespace foxglove {

FoxgloveResult<WebSocketServer> WebSocketServer::create(
  WebSocketServerOptions&& options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  bool has_any_callbacks =
    options.callbacks.onSubscribe || options.callbacks.onUnsubscribe ||
    options.callbacks.onClientAdvertise || options.callbacks.onMessageData ||
    options.callbacks.onClientUnadvertise || options.callbacks.onGetParameters ||
    options.callbacks.onSetParameters || options.callbacks.onParametersSubscribe ||
    options.callbacks.onParametersUnsubscribe || options.callbacks.onConnectionGraphSubscribe ||
    options.callbacks.onConnectionGraphUnsubscribe;

  std::unique_ptr<WebSocketServerCallbacks> callbacks;

  foxglove_server_callbacks c_callbacks = {};

  if (has_any_callbacks) {
    callbacks = std::make_unique<WebSocketServerCallbacks>(std::move(options.callbacks));
    c_callbacks.context = callbacks.get();
    if (callbacks->onSubscribe) {
      c_callbacks.on_subscribe = [](uint64_t channel_id, const void* context) {
        try {
          (static_cast<const WebSocketServerCallbacks*>(context))->onSubscribe(channel_id);
        } catch (const std::exception& exc) {
          warn() << "onSubscribe callback failed: " << exc.what();
        }
      };
    }
    if (callbacks->onUnsubscribe) {
      c_callbacks.on_unsubscribe = [](uint64_t channel_id, const void* context) {
        try {
          (static_cast<const WebSocketServerCallbacks*>(context))->onUnsubscribe(channel_id);
        } catch (const std::exception& exc) {
          warn() << "onUnsubscribe callback failed: " << exc.what();
        }
      };
    }
    if (callbacks->onClientAdvertise) {
      c_callbacks.on_client_advertise =
        [](uint32_t client_id, const foxglove_client_channel* channel, const void* context) {
          ClientChannel cpp_channel = {
            channel->id,
            channel->topic,
            channel->encoding,
            channel->schema_name,
            channel->schema_encoding == nullptr ? std::string_view{} : channel->schema_encoding,
            reinterpret_cast<const std::byte*>(channel->schema),
            channel->schema_len
          };
          try {
            (static_cast<const WebSocketServerCallbacks*>(context))
              ->onClientAdvertise(client_id, cpp_channel);
          } catch (const std::exception& exc) {
            warn() << "onClientAdvertise callback failed: " << exc.what();
          }
        };
    }
    if (callbacks->onMessageData) {
      c_callbacks.on_message_data = [](
                                      // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
                                      uint32_t client_id,
                                      uint32_t client_channel_id,
                                      const uint8_t* payload,
                                      size_t payload_len,
                                      const void* context
                                    ) {
        try {
          (static_cast<const WebSocketServerCallbacks*>(context))
            ->onMessageData(
              client_id, client_channel_id, reinterpret_cast<const std::byte*>(payload), payload_len
            );
        } catch (const std::exception& exc) {
          warn() << "onMessageData callback failed: " << exc.what();
        }
      };
    }
    if (callbacks->onClientUnadvertise) {
      c_callbacks.on_client_unadvertise =
        // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
        [](uint32_t client_id, uint32_t client_channel_id, const void* context) {
          try {
            (static_cast<const WebSocketServerCallbacks*>(context))
              ->onClientUnadvertise(client_id, client_channel_id);
          } catch (const std::exception& exc) {
            warn() << "onClientUnadvertise callback failed: " << exc.what();
          }
        };
    }
    if (callbacks->onGetParameters) {
      c_callbacks.on_get_parameters = [](
                                        const void* context,
                                        uint32_t client_id,
                                        // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
                                        const struct foxglove_string* c_request_id,
                                        const struct foxglove_string* c_param_names,
                                        size_t param_names_len
                                      ) -> foxglove_parameter_array* {
        std::optional<std::string_view> request_id;
        if (c_request_id != nullptr) {
          request_id.emplace(c_request_id->data, c_request_id->len);
        }
        std::vector<std::string_view> param_names;
        if (c_param_names != nullptr) {
          param_names.reserve(param_names_len);
          for (auto i = 0; i < param_names_len; ++i) {
            param_names.emplace_back(c_param_names[i].data, c_param_names[i].len);
          }
        }
        std::vector<foxglove::Parameter> params;
        try {
          params = (static_cast<const WebSocketServerCallbacks*>(context))
                     ->onGetParameters(client_id, request_id, param_names);
        } catch (const std::exception& exc) {
          warn() << "onGetParameters callback failed: " << exc.what();
        }
        auto array = ParameterArray(std::move(params));
        return array.release();
      };
    }
    if (callbacks->onSetParameters) {
      c_callbacks.on_set_parameters = [](
                                        const void* context,
                                        uint32_t client_id,
                                        const struct foxglove_string* c_request_id,
                                        const foxglove_parameter_array* c_params
                                      ) -> foxglove_parameter_array* {
        std::optional<std::string_view> request_id;
        if (c_request_id != nullptr) {
          request_id.emplace(c_request_id->data, c_request_id->len);
        }
        if (c_params == nullptr) {
          // Should not happen; the C implementation never passes a null pointer.
          return nullptr;
        }
        std::vector<foxglove::Parameter> params;
        try {
          params =
            (static_cast<const WebSocketServerCallbacks*>(context))
              ->onSetParameters(client_id, request_id, ParameterArrayView(c_params).parameters());
        } catch (const std::exception& exc) {
          warn() << "onSetParameters callback failed: " << exc.what();
        }
        auto array = ParameterArray(std::move(params));
        return array.release();
      };
    }
    if (callbacks->onParametersSubscribe) {
      c_callbacks.on_parameters_subscribe =
        [](const void* context, const struct foxglove_string* c_names, size_t names_len) {
          std::vector<std::string_view> names;
          names.reserve(names_len);
          for (auto i = 0; i < names_len; ++i) {
            names.emplace_back(c_names[i].data, c_names[i].len);
          }
          try {
            (static_cast<const WebSocketServerCallbacks*>(context))->onParametersSubscribe(names);
          } catch (const std::exception& exc) {
            warn() << "onParametersSubscribe callback failed: " << exc.what();
          }
        };
    }
    if (callbacks->onParametersUnsubscribe) {
      c_callbacks.on_parameters_unsubscribe =
        [](const void* context, const struct foxglove_string* c_names, size_t names_len) {
          std::vector<std::string_view> names;
          names.reserve(names_len);
          for (auto i = 0; i < names_len; ++i) {
            names.emplace_back(c_names[i].data, c_names[i].len);
          }
          try {
            (static_cast<const WebSocketServerCallbacks*>(context))->onParametersUnsubscribe(names);
          } catch (const std::exception& exc) {
            warn() << "onParametersUnsubscribe callback failed: " << exc.what();
          }
        };
    }
    if (callbacks->onConnectionGraphSubscribe) {
      c_callbacks.on_connection_graph_subscribe = [](const void* context) {
        try {
          (static_cast<const WebSocketServerCallbacks*>(context))->onConnectionGraphSubscribe();
        } catch (const std::exception& exc) {
          warn() << "onConnectionGraphSubscribe callback failed: " << exc.what();
        }
      };
    }
    if (callbacks->onConnectionGraphUnsubscribe) {
      c_callbacks.on_connection_graph_unsubscribe = [](const void* context) {
        try {
          (static_cast<const WebSocketServerCallbacks*>(context))->onConnectionGraphUnsubscribe();
        } catch (const std::exception& exc) {
          warn() << "onConnectionGraphUnsubscribe callback failed: " << exc.what();
        }
      };
    }
  }

  foxglove_server_options c_cptions = {};
  c_cptions.context = options.context.getInner();
  c_cptions.name = {options.name.c_str(), options.name.length()};
  c_cptions.host = {options.host.c_str(), options.host.length()};
  c_cptions.port = options.port;
  c_cptions.callbacks = has_any_callbacks ? &c_callbacks : nullptr;
  c_cptions.capabilities =
    static_cast<std::underlying_type_t<decltype(options.capabilities)>>(options.capabilities);
  std::vector<foxglove_string> supported_encodings;
  supported_encodings.reserve(options.supported_encodings.size());
  for (const auto& encoding : options.supported_encodings) {
    supported_encodings.push_back({encoding.c_str(), encoding.length()});
  }
  c_cptions.supported_encodings = supported_encodings.data();
  c_cptions.supported_encodings_count = supported_encodings.size();

  foxglove_websocket_server* server = nullptr;
  foxglove_error error = foxglove_server_start(&c_cptions, &server);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || server == nullptr) {
    return foxglove::unexpected(static_cast<FoxgloveError>(error));
  }

  return WebSocketServer(server, std::move(callbacks));
}

WebSocketServer::WebSocketServer(
  foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks
)
    : impl_(server, foxglove_server_stop)
    , callbacks_(std::move(callbacks)) {}

FoxgloveError WebSocketServer::stop() {
  foxglove_error error = foxglove_server_stop(impl_.release());
  return FoxgloveError(error);
}

uint16_t WebSocketServer::port() const {
  return foxglove_server_get_port(impl_.get());
}

// NOLINTNEXTLINE(cppcoreguidelines-rvalue-reference-param-not-moved)
FoxgloveError WebSocketServer::addService(Service&& service) const noexcept {
  auto error = foxglove_server_add_service(impl_.get(), service.release());
  return FoxgloveError(error);
}

FoxgloveError WebSocketServer::removeService(std::string_view name) const noexcept {
  auto error = foxglove_server_remove_service(impl_.get(), {name.data(), name.length()});
  return FoxgloveError(error);
}

void WebSocketServer::publishParameterValues(std::vector<Parameter>&& params) {
  ParameterArray array(std::move(params));
  foxglove_server_publish_parameter_values(impl_.get(), array.release());
}

void WebSocketServer::publishConnectionGraph(ConnectionGraph& graph) {
  foxglove_server_publish_connection_graph(impl_.get(), graph.impl_.get());
}

}  // namespace foxglove
