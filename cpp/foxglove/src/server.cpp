#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>

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
  std::unique_ptr<FetchAssetHandler> fetch_asset;

  foxglove_server_callbacks c_callbacks = {};

  if (has_any_callbacks) {
    callbacks = std::make_unique<WebSocketServerCallbacks>(std::move(options.callbacks));
    c_callbacks.context = callbacks.get();
    if (callbacks->onSubscribe) {
      c_callbacks.on_subscribe = [](
                                   const void* context,
                                   uint64_t channel_id,
                                   const foxglove_client_metadata c_client_metadata
                                 ) {
        try {
          ClientMetadata client_metadata{
            c_client_metadata.id,
            c_client_metadata.sink_id == 0 ? std::nullopt
                                           : std::make_optional<uint64_t>(c_client_metadata.sink_id)
          };
          (static_cast<const WebSocketServerCallbacks*>(context))
            ->onSubscribe(channel_id, client_metadata);
        } catch (const std::exception& exc) {
          warn() << "onSubscribe callback failed: " << exc.what();
        }
      };
    }
    if (callbacks->onUnsubscribe) {
      c_callbacks.on_unsubscribe =
        [](const void* context, uint64_t channel_id, foxglove_client_metadata c_client_metadata) {
          try {
            ClientMetadata client_metadata{
              c_client_metadata.id,
              c_client_metadata.sink_id == 0
                ? std::nullopt
                : std::make_optional<uint64_t>(c_client_metadata.sink_id)
            };
            (static_cast<const WebSocketServerCallbacks*>(context))
              ->onUnsubscribe(channel_id, client_metadata);
          } catch (const std::exception& exc) {
            warn() << "onUnsubscribe callback failed: " << exc.what();
          }
        };
    }
    if (callbacks->onClientAdvertise) {
      c_callbacks.on_client_advertise =
        [](const void* context, uint32_t client_id, const foxglove_client_channel* channel) {
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
                                      const void* context,
                                      // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
                                      uint32_t client_id,
                                      uint32_t client_channel_id,
                                      const uint8_t* payload,
                                      size_t payload_len
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
          for (size_t i = 0; i < param_names_len; ++i) {
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
          for (size_t i = 0; i < names_len; ++i) {
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
          for (size_t i = 0; i < names_len; ++i) {
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

  foxglove_server_options c_options = {};
  c_options.context = options.context.getInner();
  c_options.name = {options.name.c_str(), options.name.length()};
  c_options.host = {options.host.c_str(), options.host.length()};
  c_options.port = options.port;
  c_options.callbacks = has_any_callbacks ? &c_callbacks : nullptr;
  c_options.capabilities =
    static_cast<std::underlying_type_t<decltype(options.capabilities)>>(options.capabilities);
  std::vector<foxglove_string> supported_encodings;
  supported_encodings.reserve(options.supported_encodings.size());
  for (const auto& encoding : options.supported_encodings) {
    supported_encodings.push_back({encoding.c_str(), encoding.length()});
  }
  c_options.supported_encodings = supported_encodings.data();
  c_options.supported_encodings_count = supported_encodings.size();
  if (options.fetch_asset) {
    fetch_asset = std::make_unique<FetchAssetHandler>(options.fetch_asset);
    c_options.fetch_asset_context = fetch_asset.get();
    c_options.fetch_asset = [](
                              const void* context,
                              const struct foxglove_string* c_uri,
                              struct foxglove_fetch_asset_responder* c_responder
                            ) {
      try {
        const auto* handler = static_cast<const FetchAssetHandler*>(context);
        std::string_view uri{c_uri->data, c_uri->len};
        FetchAssetResponder responder(c_responder);
        (*handler)(uri, std::move(responder));
      } catch (const std::exception& exc) {
        warn() << "Fetch asset callback failed: " << exc.what();
      }
    };
  }

  foxglove_websocket_server* server = nullptr;
  foxglove_error error = foxglove_server_start(&c_options, &server);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || server == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return WebSocketServer(server, std::move(callbacks), std::move(fetch_asset));
}

WebSocketServer::WebSocketServer(
  foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks,
  std::unique_ptr<FetchAssetHandler> fetch_asset
)
    : callbacks_(std::move(callbacks))
    , fetch_asset_(std::move(fetch_asset))
    , impl_(server, foxglove_server_stop) {}

FoxgloveError WebSocketServer::stop() {
  foxglove_error error = foxglove_server_stop(impl_.release());
  return FoxgloveError(error);
}

uint16_t WebSocketServer::port() const {
  return foxglove_server_get_port(impl_.get());
}

void WebSocketServer::broadcastTime(uint64_t timestamp_nanos) const noexcept {
  foxglove_server_broadcast_time(impl_.get(), timestamp_nanos);
}

FoxgloveError WebSocketServer::clearSession(std::optional<std::string_view> session_id
) const noexcept {
  auto c_session_id = session_id
                        ? std::optional<foxglove_string>{{session_id->data(), session_id->size()}}
                        : std::nullopt;
  auto error = foxglove_server_clear_session(impl_.get(), c_session_id ? &*c_session_id : nullptr);
  return FoxgloveError(error);
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

FoxgloveError WebSocketServer::publishStatus(
  WebSocketServerStatusLevel level, std::string_view message, std::optional<std::string_view> id
) const noexcept {
  auto c_id = id ? std::optional<foxglove_string>{{id->data(), id->size()}} : std::nullopt;
  auto error = foxglove_server_publish_status(
    impl_.get(),
    static_cast<foxglove_server_status_level>(level),
    {message.data(), message.size()},
    c_id ? &*c_id : nullptr
  );
  return FoxgloveError(error);
}

FoxgloveError WebSocketServer::removeStatus(const std::vector<std::string_view>& ids) const {
  std::vector<foxglove_string> c_ids;
  c_ids.reserve(ids.size());
  for (const auto& id : ids) {
    c_ids.push_back({id.data(), id.size()});
  }
  auto error = foxglove_server_remove_status(impl_.get(), c_ids.data(), c_ids.size());
  return FoxgloveError(error);
}

}  // namespace foxglove
