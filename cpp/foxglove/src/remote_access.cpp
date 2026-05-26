// The build system normally defines FOXGLOVE_REMOTE_ACCESS for us — e.g.,
// foxglove_sdk_add_cpp_library() in the dist's cmake config, or the in-tree
// foxglove_cpp_shared target. The guard lets this file still compile cleanly if it's
// invoked outside cmake (or by a consumer that hand-rolls the wrapper build), where
// foxglove-c/foxglove-c.h's RA-guarded declarations would otherwise be invisible.
#ifndef FOXGLOVE_REMOTE_ACCESS
#define FOXGLOVE_REMOTE_ACCESS
#endif
#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/remote_access.hpp>

#include "callback_forwarders.hpp"

namespace foxglove {
namespace {

void forwardOnConnectionStatusChanged(const void* context, foxglove_connection_status status) {
  internal::callbackGuard("onConnectionStatusChanged", [&] {
    static_cast<const RemoteAccessGatewayCallbacks*>(context)->onConnectionStatusChanged(
      static_cast<RemoteAccessConnectionStatus>(status)
    );
  });
}

void forwardOnSubscribe(
  const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel
) {
  internal::callbackGuard("onSubscribe", [&] {
    auto cpp_channel = ChannelDescriptor(channel);
    static_cast<const RemoteAccessGatewayCallbacks*>(context)->onSubscribe(client_id, cpp_channel);
  });
}

void forwardOnUnsubscribe(
  const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel
) {
  internal::callbackGuard("onUnsubscribe", [&] {
    auto cpp_channel = ChannelDescriptor(channel);
    static_cast<const RemoteAccessGatewayCallbacks*>(context)->onUnsubscribe(
      client_id, cpp_channel
    );
  });
}

void forwardOnMessageData(
  const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel,
  const uint8_t* payload, size_t payload_len
) {
  internal::callbackGuard("onMessageData", [&] {
    auto cpp_channel = ChannelDescriptor(channel);
    static_cast<const RemoteAccessGatewayCallbacks*>(context)->onMessageData(
      client_id, cpp_channel, reinterpret_cast<const std::byte*>(payload), payload_len
    );
  });
}

void forwardOnClientAdvertise(
  const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel
) {
  internal::callbackGuard("onClientAdvertise", [&] {
    auto cpp_channel = ChannelDescriptor(channel);
    static_cast<const RemoteAccessGatewayCallbacks*>(context)->onClientAdvertise(
      client_id, cpp_channel
    );
  });
}

void forwardOnClientUnadvertise(
  const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel
) {
  internal::callbackGuard("onClientUnadvertise", [&] {
    auto cpp_channel = ChannelDescriptor(channel);
    static_cast<const RemoteAccessGatewayCallbacks*>(context)->onClientUnadvertise(
      client_id, cpp_channel
    );
  });
}

foxglove_qos_profile forwardQosClassifier(
  const void* context, const foxglove_channel_descriptor* channel
) {
  if (context == nullptr) {
    return {FOXGLOVE_RELIABILITY_LOSSY};
  }
  try {
    const auto* classifier = static_cast<const QosClassifierFn*>(context);
    auto cpp_channel = ChannelDescriptor(channel);
    auto profile = (*classifier)(cpp_channel);
    return {static_cast<foxglove_reliability>(profile.reliability)};
  } catch (const std::exception& exc) {
    warn() << "QoS classifier failed: " << exc.what();
    return {FOXGLOVE_RELIABILITY_LOSSY};
  }
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
bool wireGatewayCallbacks(foxglove_gateway_callbacks& c, const RemoteAccessGatewayCallbacks& cb) {
  bool any = false;
  if (cb.onConnectionStatusChanged) {
    c.on_connection_status_changed = &forwardOnConnectionStatusChanged;
    any = true;
  }
  if (cb.onSubscribe) {
    c.on_subscribe = &forwardOnSubscribe;
    any = true;
  }
  if (cb.onUnsubscribe) {
    c.on_unsubscribe = &forwardOnUnsubscribe;
    any = true;
  }
  if (cb.onMessageData) {
    c.on_message_data = &forwardOnMessageData;
    any = true;
  }
  if (cb.onClientAdvertise) {
    c.on_client_advertise = &forwardOnClientAdvertise;
    any = true;
  }
  if (cb.onClientUnadvertise) {
    c.on_client_unadvertise = &forwardOnClientUnadvertise;
    any = true;
  }
  if (cb.onGetParameters) {
    c.on_get_parameters = &internal::forwardLegacyGetParameters<RemoteAccessGatewayCallbacks>;
    any = true;
  }
  if (cb.onSetParameters) {
    c.on_set_parameters = &internal::forwardLegacySetParameters<RemoteAccessGatewayCallbacks>;
    any = true;
  }
  if (cb.onParametersSubscribe) {
    c.on_parameters_subscribe = &internal::forwardParametersSubscribe<RemoteAccessGatewayCallbacks>;
    any = true;
  }
  if (cb.onParametersUnsubscribe) {
    c.on_parameters_unsubscribe =
      &internal::forwardParametersUnsubscribe<RemoteAccessGatewayCallbacks>;
    any = true;
  }
  if (cb.onConnectionGraphSubscribe) {
    c.on_connection_graph_subscribe =
      &internal::forwardConnectionGraphSubscribe<RemoteAccessGatewayCallbacks>;
    any = true;
  }
  if (cb.onConnectionGraphUnsubscribe) {
    c.on_connection_graph_unsubscribe =
      &internal::forwardConnectionGraphUnsubscribe<RemoteAccessGatewayCallbacks>;
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

FoxgloveResult<RemoteAccessGateway> RemoteAccessGateway::create(
  RemoteAccessGatewayOptions&&
    options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_gateway_callbacks c_callbacks = {};
  std::unique_ptr<RemoteAccessGatewayCallbacks> callbacks;
  if (wireGatewayCallbacks(c_callbacks, options.callbacks)) {
    callbacks = std::make_unique<RemoteAccessGatewayCallbacks>(std::move(options.callbacks));
    c_callbacks.context = callbacks.get();
  }

  std::unique_ptr<FetchAssetHandler> fetch_asset;
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter;
  std::unique_ptr<ParameterHandler> parameter_handler;

  // Build C options
  foxglove_gateway_options c_options = {};
  c_options.context = options.context.getInner();
  c_options.name = {options.name.c_str(), options.name.length()};
  c_options.device_token = {options.device_token.c_str(), options.device_token.length()};
  c_options.callbacks = callbacks ? &c_callbacks : nullptr;
  c_options.capabilities = {
    static_cast<std::underlying_type_t<decltype(options.capabilities)>>(options.capabilities)
  };

  // Supported encodings
  std::vector<foxglove_string> supported_encodings;
  supported_encodings.reserve(options.supported_encodings.size());
  for (const auto& encoding : options.supported_encodings) {
    supported_encodings.push_back({encoding.c_str(), encoding.length()});
  }
  c_options.supported_encodings = supported_encodings.data();
  c_options.supported_encodings_count = supported_encodings.size();

  // Sink channel filter
  internal::wireSinkChannelFilter(
    c_options, std::move(options.sink_channel_filter), sink_channel_filter
  );

  // QoS classifier
  std::unique_ptr<QosClassifierFn> qos_classifier;
  if (options.qos_classifier) {
    qos_classifier = std::make_unique<QosClassifierFn>(std::move(options.qos_classifier));
    c_options.qos_classifier_context = qos_classifier.get();
    c_options.qos_classifier = &forwardQosClassifier;
  }

  // Fetch asset handler
  internal::wireFetchAsset(c_options, std::move(options.fetch_asset), fetch_asset);

  foxglove_parameter_handler c_parameter_handler = {};
  if (auto err = internal::wireParameterHandler(
        c_options, c_parameter_handler, std::move(options.parameter_handler), parameter_handler
      );
      err != FoxgloveError::Ok) {
    return tl::unexpected(err);
  }

  // Optional API URL
  foxglove_string api_url = {};
  if (options.foxglove_api_url) {
    api_url = {options.foxglove_api_url->c_str(), options.foxglove_api_url->length()};
    c_options.foxglove_api_url = api_url;
  }

  // Optional timeout
  if (options.foxglove_api_timeout_secs) {
    c_options.foxglove_api_timeout_secs = &*options.foxglove_api_timeout_secs;
  }

  c_options.message_backlog_size = options.message_backlog_size.value_or(0);

  std::vector<foxglove_key_value> server_info;
  if (options.server_info) {
    server_info.reserve(options.server_info->size());
    for (auto const& [key, value] : *options.server_info) {
      server_info.push_back({{key.data(), key.length()}, {value.data(), value.length()}});
    }
    c_options.server_info = server_info.data();
    c_options.server_info_count = server_info.size();
  }

  foxglove_gateway* gateway = nullptr;
  foxglove_error error = foxglove_gateway_start(&c_options, &gateway);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || gateway == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return RemoteAccessGateway(
    gateway,
    std::move(callbacks),
    std::move(fetch_asset),
    std::move(sink_channel_filter),
    std::move(qos_classifier),
    std::move(parameter_handler)
  );
}

RemoteAccessGateway::RemoteAccessGateway(
  foxglove_gateway* gateway, std::unique_ptr<RemoteAccessGatewayCallbacks> callbacks,
  std::unique_ptr<FetchAssetHandler> fetch_asset,
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter,
  std::unique_ptr<QosClassifierFn> qos_classifier,
  std::unique_ptr<ParameterHandler> parameter_handler
)
    : callbacks_(std::move(callbacks))
    , fetch_asset_(std::move(fetch_asset))
    , sink_channel_filter_(std::move(sink_channel_filter))
    , qos_classifier_(std::move(qos_classifier))
    , parameter_handler_(std::move(parameter_handler))
    , impl_(gateway, foxglove_gateway_stop) {}

RemoteAccessConnectionStatus RemoteAccessGateway::connectionStatus() const {
  return static_cast<RemoteAccessConnectionStatus>(foxglove_gateway_connection_status(impl_.get()));
}

// NOLINTNEXTLINE(cppcoreguidelines-rvalue-reference-param-not-moved)
FoxgloveError RemoteAccessGateway::addService(Service&& service) const noexcept {
  auto error = foxglove_gateway_add_service(impl_.get(), service.release());
  return FoxgloveError(error);
}

FoxgloveError RemoteAccessGateway::removeService(std::string_view name) const noexcept {
  foxglove_string c_name = {name.data(), name.length()};
  auto error = foxglove_gateway_remove_service(impl_.get(), c_name);
  return FoxgloveError(error);
}

void RemoteAccessGateway::publishParameterValues(std::vector<Parameter>&& params) {
  ParameterArray array(std::move(params));
  foxglove_gateway_publish_parameter_values(impl_.get(), array.release());
}

FoxgloveError RemoteAccessGateway::publishStatus(
  RemoteAccessStatusLevel level, std::string_view message, std::optional<std::string_view> id
) const noexcept {
  auto c_id = id ? std::optional<foxglove_string>{{id->data(), id->size()}} : std::nullopt;
  auto error = foxglove_gateway_publish_status(
    impl_.get(),
    static_cast<foxglove_server_status_level>(level),
    {message.data(), message.size()},
    c_id ? &*c_id : nullptr
  );
  return FoxgloveError(error);
}

FoxgloveError RemoteAccessGateway::removeStatus(const std::vector<std::string_view>& ids) const {
  std::vector<foxglove_string> c_ids;
  c_ids.reserve(ids.size());
  for (const auto& id : ids) {
    c_ids.push_back({id.data(), id.size()});
  }
  auto error = foxglove_gateway_remove_status(impl_.get(), c_ids.data(), c_ids.size());
  return FoxgloveError(error);
}

FoxgloveError RemoteAccessGateway::publishConnectionGraph(const ConnectionGraph& graph) const {
  return FoxgloveError(foxglove_gateway_publish_connection_graph(impl_.get(), graph.impl_.get()));
}

std::optional<uint64_t> RemoteAccessGateway::sinkId() const {
  uint64_t id = foxglove_gateway_sink_id(impl_.get());
  if (id == 0) {
    return std::nullopt;
  }
  return id;
}

FoxgloveError RemoteAccessGateway::stop() {
  foxglove_error error = foxglove_gateway_stop(impl_.release());
  return FoxgloveError(error);
}

}  // namespace foxglove
