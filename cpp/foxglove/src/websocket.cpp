#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/websocket.hpp>

#include <type_traits>

#include "callback_forwarders.hpp"

namespace foxglove {
namespace {

void forwardOnSubscribe(
  const void* context, uint64_t channel_id, foxglove_client_metadata c_client_metadata
) {
  internal::callbackGuard("onSubscribe", [&] {
    ClientMetadata client_metadata{
      c_client_metadata.id,
      c_client_metadata.sink_id == 0 ? std::nullopt
                                     : std::make_optional<uint64_t>(c_client_metadata.sink_id)
    };
    static_cast<const WebSocketServerCallbacks*>(context)->onSubscribe(channel_id, client_metadata);
  });
}

void forwardOnUnsubscribe(
  const void* context, uint64_t channel_id, foxglove_client_metadata c_client_metadata
) {
  internal::callbackGuard("onUnsubscribe", [&] {
    ClientMetadata client_metadata{
      c_client_metadata.id,
      c_client_metadata.sink_id == 0 ? std::nullopt
                                     : std::make_optional<uint64_t>(c_client_metadata.sink_id)
    };
    static_cast<const WebSocketServerCallbacks*>(context)->onUnsubscribe(
      channel_id, client_metadata
    );
  });
}

void forwardOnClientAdvertise(
  const void* context, uint32_t client_id, const foxglove_client_channel* channel
) {
  internal::callbackGuard("onClientAdvertise", [&] {
    ClientChannel cpp_channel = {
      channel->id,
      channel->topic,
      channel->encoding,
      channel->schema_name,
      channel->schema_encoding == nullptr ? std::string_view{} : channel->schema_encoding,
      reinterpret_cast<const std::byte*>(channel->schema),
      channel->schema_len
    };
    static_cast<const WebSocketServerCallbacks*>(context)->onClientAdvertise(
      client_id, cpp_channel
    );
  });
}

void forwardOnMessageData(
  const void* context, uint32_t client_id, uint32_t client_channel_id, const uint8_t* payload,
  size_t payload_len
) {
  internal::callbackGuard("onMessageData", [&] {
    static_cast<const WebSocketServerCallbacks*>(context)->onMessageData(
      client_id, client_channel_id, reinterpret_cast<const std::byte*>(payload), payload_len
    );
  });
}

// The C ABI signature for on_client_unadvertise places context after the ids,
// not first; we preserve that to match the C header.
void forwardOnClientUnadvertise(
  uint32_t client_id, uint32_t client_channel_id, const void* context
) {
  internal::callbackGuard("onClientUnadvertise", [&] {
    static_cast<const WebSocketServerCallbacks*>(context)->onClientUnadvertise(
      client_id, client_channel_id
    );
  });
}

void forwardOnClientConnect(const void* context) {
  internal::callbackGuard("onClientConnect", [&] {
    static_cast<const WebSocketServerCallbacks*>(context)->onClientConnect();
  });
}

void forwardOnClientDisconnect(const void* context) {
  internal::callbackGuard("onClientDisconnect", [&] {
    static_cast<const WebSocketServerCallbacks*>(context)->onClientDisconnect();
  });
}

void forwardOnPlaybackControlRequest(
  const void* context, const foxglove_playback_control_request* c_request,
  foxglove_playback_state* c_state
) {
  if (c_request == nullptr) {
    return;
  }
  std::optional<PlaybackState> maybe_state;
  internal::callbackGuard("onPlaybackControlRequest", [&] {
    const auto* ctx = static_cast<const WebSocketServerCallbacks*>(context);
    maybe_state = ctx->onPlaybackControlRequest(PlaybackControlRequest::from(*c_request));
  });
  if (!maybe_state.has_value()) {
    return;
  }
  const auto& state = *maybe_state;
  c_state->status = static_cast<uint8_t>(state.status);
  c_state->current_time = state.current_time;
  c_state->playback_speed = state.playback_speed;
  c_state->did_seek = state.did_seek;
}

// Populates `c` with forward function pointers for every callback set on `cb`,
// and reports whether any callback was set. Callers should leave both the
// heap-allocated callbacks wrapper unallocated and `c_options.callbacks` null
// when this returns false: the Rust side skips registering a listener entirely
// for a null callbacks pointer, so we want to match that "no listener at all"
// path rather than registering an empty one. `c.context` is left untouched;
// the caller sets it once it has constructed the wrapper.
//
// The legacy onGetParameters/onSetParameters fields are [[deprecated]] for
// consumers, but this file is the internal C-ABI forward and must keep
// reading them.
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#elif defined(_MSC_VER)
#pragma warning(push)
#pragma warning(disable : 4996)
#endif
bool wireServerCallbacks(foxglove_server_callbacks& c, const WebSocketServerCallbacks& cb) {
  bool any = false;
  if (cb.onSubscribe) {
    c.on_subscribe = &forwardOnSubscribe;
    any = true;
  }
  if (cb.onUnsubscribe) {
    c.on_unsubscribe = &forwardOnUnsubscribe;
    any = true;
  }
  if (cb.onClientAdvertise) {
    c.on_client_advertise = &forwardOnClientAdvertise;
    any = true;
  }
  if (cb.onMessageData) {
    c.on_message_data = &forwardOnMessageData;
    any = true;
  }
  if (cb.onClientUnadvertise) {
    c.on_client_unadvertise = &forwardOnClientUnadvertise;
    any = true;
  }
  if (cb.onGetParameters) {
    c.on_get_parameters = &internal::forwardLegacyGetParameters<WebSocketServerCallbacks>;
    any = true;
  }
  if (cb.onSetParameters) {
    c.on_set_parameters = &internal::forwardLegacySetParameters<WebSocketServerCallbacks>;
    any = true;
  }
  if (cb.onParametersSubscribe) {
    c.on_parameters_subscribe = &internal::forwardParametersSubscribe<WebSocketServerCallbacks>;
    any = true;
  }
  if (cb.onParametersUnsubscribe) {
    c.on_parameters_unsubscribe = &internal::forwardParametersUnsubscribe<WebSocketServerCallbacks>;
    any = true;
  }
  if (cb.onConnectionGraphSubscribe) {
    c.on_connection_graph_subscribe =
      &internal::forwardConnectionGraphSubscribe<WebSocketServerCallbacks>;
    any = true;
  }
  if (cb.onConnectionGraphUnsubscribe) {
    c.on_connection_graph_unsubscribe =
      &internal::forwardConnectionGraphUnsubscribe<WebSocketServerCallbacks>;
    any = true;
  }
  if (cb.onClientConnect) {
    c.on_client_connect = &forwardOnClientConnect;
    any = true;
  }
  if (cb.onClientDisconnect) {
    c.on_client_disconnect = &forwardOnClientDisconnect;
    any = true;
  }
  if (cb.onPlaybackControlRequest) {
    c.on_playback_control_request = &forwardOnPlaybackControlRequest;
    any = true;
  }
  return any;
}
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic pop
#elif defined(_MSC_VER)
#pragma warning(pop)
#endif

}  // namespace

FoxgloveResult<WebSocketServer> WebSocketServer::create(
  WebSocketServerOptions&& options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_server_callbacks c_callbacks = {};
  std::unique_ptr<WebSocketServerCallbacks> callbacks;
  if (wireServerCallbacks(c_callbacks, options.callbacks)) {
    callbacks = std::make_unique<WebSocketServerCallbacks>(std::move(options.callbacks));
    c_callbacks.context = callbacks.get();
  }

  std::unique_ptr<FetchAssetHandler> fetch_asset;
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter;
  std::unique_ptr<ParameterHandler> parameter_handler;

  foxglove_server_options c_options = {};
  c_options.context = options.context.getInner();
  c_options.name = {options.name.c_str(), options.name.length()};
  c_options.host = {options.host.c_str(), options.host.length()};
  c_options.port = options.port;
  c_options.callbacks = callbacks ? &c_callbacks : nullptr;
  c_options.capabilities =
    static_cast<std::underlying_type_t<decltype(options.capabilities)>>(options.capabilities);
  std::vector<foxglove_string> supported_encodings;
  supported_encodings.reserve(options.supported_encodings.size());
  for (const auto& encoding : options.supported_encodings) {
    supported_encodings.push_back({encoding.c_str(), encoding.length()});
  }
  c_options.supported_encodings = supported_encodings.data();
  c_options.supported_encodings_count = supported_encodings.size();
  internal::wireFetchAsset(c_options, std::move(options.fetch_asset), fetch_asset);

  if (options.playback_time_range) {
    c_options.playback_start_time = &options.playback_time_range->first;
    c_options.playback_end_time = &options.playback_time_range->second;
  }

  foxglove_parameter_handler c_parameter_handler = {};
  if (auto err = internal::wireParameterHandler(
        c_options, c_parameter_handler, std::move(options.parameter_handler), parameter_handler
      );
      err != FoxgloveError::Ok) {
    return tl::unexpected(err);
  }

  std::vector<foxglove_key_value> server_info;
  if (options.server_info) {
    server_info.reserve(options.server_info->size());
    for (auto const& [key, value] : *options.server_info) {
      server_info.push_back({{key.data(), key.length()}, {value.data(), value.length()}});
    }
    c_options.server_info = server_info.data();
    c_options.server_info_count = server_info.size();
  }

  if (options.tls_identity) {
    c_options.tls_cert = reinterpret_cast<const uint8_t*>(options.tls_identity->cert.data());
    c_options.tls_cert_len = options.tls_identity->cert.size();
    c_options.tls_key = reinterpret_cast<const uint8_t*>(options.tls_identity->key.data());
    c_options.tls_key_len = options.tls_identity->key.size();
  }

  internal::wireSinkChannelFilter(
    c_options, std::move(options.sink_channel_filter), sink_channel_filter
  );

  std::optional<foxglove_string> session_id;
  if (options.session_id) {
    session_id = foxglove_string{options.session_id->data(), options.session_id->length()};
    c_options.session_id = &*session_id;
  }

  c_options.message_backlog_size = options.message_backlog_size.value_or(0);

  foxglove_websocket_server* server = nullptr;
  foxglove_error error = foxglove_server_start(&c_options, &server);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || server == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return WebSocketServer(
    server,
    std::move(callbacks),
    std::move(fetch_asset),
    std::move(sink_channel_filter),
    std::move(parameter_handler)
  );
}

WebSocketServer::WebSocketServer(
  foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks,
  std::unique_ptr<FetchAssetHandler> fetch_asset,
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter,
  std::unique_ptr<ParameterHandler> parameter_handler
)
    : callbacks_(std::move(callbacks))
    , fetch_asset_(std::move(fetch_asset))
    , sink_channel_filter_(std::move(sink_channel_filter))
    , parameter_handler_(std::move(parameter_handler))
    , impl_(server, foxglove_server_stop) {}

FoxgloveError WebSocketServer::stop() {
  foxglove_error error = foxglove_server_stop(impl_.release());
  return FoxgloveError(error);
}

uint16_t WebSocketServer::port() const {
  return foxglove_server_get_port(impl_.get());
}

size_t WebSocketServer::clientCount() const {
  return foxglove_server_get_client_count(impl_.get());
}

void WebSocketServer::broadcastTime(uint64_t timestamp_nanos) const noexcept {
  foxglove_server_broadcast_time(impl_.get(), timestamp_nanos);
}

/// @cond foxglove_internal
void WebSocketServer::broadcastPlaybackState(const PlaybackState& playback_state) const noexcept {
  foxglove_playback_state c_playback_state_ptr;
  c_playback_state_ptr.status = static_cast<uint8_t>(playback_state.status);
  c_playback_state_ptr.current_time = playback_state.current_time;
  c_playback_state_ptr.playback_speed = playback_state.playback_speed;
  c_playback_state_ptr.did_seek = playback_state.did_seek;
  if (!playback_state.request_id.has_value() || playback_state.request_id->empty()) {
    c_playback_state_ptr.request_id = {nullptr, 0};
  } else {
    c_playback_state_ptr.request_id = {
      playback_state.request_id->data(), playback_state.request_id->length()
    };
  }
  foxglove_server_broadcast_playback_state(impl_.get(), &c_playback_state_ptr);
}
/// @endcond

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
