#define FOXGLOVE_REMOTE_ACCESS
#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/remote_access.hpp>

namespace foxglove {

FoxgloveResult<RemoteAccessGateway> RemoteAccessGateway::create(
  RemoteAccessGatewayOptions&&
    options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  bool has_any_callbacks = options.callbacks.onConnectionStatusChanged ||
                           options.callbacks.onSubscribe || options.callbacks.onUnsubscribe ||
                           options.callbacks.onMessageData || options.callbacks.onClientAdvertise ||
                           options.callbacks.onClientUnadvertise;

  std::unique_ptr<RemoteAccessGatewayCallbacks> callbacks;
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter;

  foxglove_gateway_callbacks c_callbacks = {};

  if (has_any_callbacks) {
    callbacks = std::make_unique<RemoteAccessGatewayCallbacks>(std::move(options.callbacks));
    c_callbacks.context = callbacks.get();

    if (callbacks->onConnectionStatusChanged) {
      c_callbacks.on_connection_status_changed =
        [](const void* context, foxglove_connection_status status) {
          try {
            (static_cast<const RemoteAccessGatewayCallbacks*>(context))
              ->onConnectionStatusChanged(static_cast<RemoteAccessConnectionStatus>(status));
          } catch (const std::exception& exc) {
            warn() << "onConnectionStatusChanged callback failed: " << exc.what();
          }
        };
    }

    if (callbacks->onSubscribe) {
      c_callbacks.on_subscribe =
        [](const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel) {
          try {
            auto cpp_channel = ChannelDescriptor(channel);
            (static_cast<const RemoteAccessGatewayCallbacks*>(context))
              ->onSubscribe(client_id, cpp_channel);
          } catch (const std::exception& exc) {
            warn() << "onSubscribe callback failed: " << exc.what();
          }
        };
    }

    if (callbacks->onUnsubscribe) {
      c_callbacks.on_unsubscribe =
        [](const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel) {
          try {
            auto cpp_channel = ChannelDescriptor(channel);
            (static_cast<const RemoteAccessGatewayCallbacks*>(context))
              ->onUnsubscribe(client_id, cpp_channel);
          } catch (const std::exception& exc) {
            warn() << "onUnsubscribe callback failed: " << exc.what();
          }
        };
    }

    if (callbacks->onMessageData) {
      c_callbacks.on_message_data = [](
                                      const void* context,
                                      uint32_t client_id,
                                      const foxglove_channel_descriptor* channel,
                                      const uint8_t* payload,
                                      size_t payload_len
                                    ) {
        try {
          auto cpp_channel = ChannelDescriptor(channel);
          (static_cast<const RemoteAccessGatewayCallbacks*>(context))
            ->onMessageData(
              client_id, cpp_channel, reinterpret_cast<const std::byte*>(payload), payload_len
            );
        } catch (const std::exception& exc) {
          warn() << "onMessageData callback failed: " << exc.what();
        }
      };
    }

    if (callbacks->onClientAdvertise) {
      c_callbacks.on_client_advertise =
        [](const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel) {
          try {
            auto cpp_channel = ChannelDescriptor(channel);
            (static_cast<const RemoteAccessGatewayCallbacks*>(context))
              ->onClientAdvertise(client_id, cpp_channel);
          } catch (const std::exception& exc) {
            warn() << "onClientAdvertise callback failed: " << exc.what();
          }
        };
    }

    if (callbacks->onClientUnadvertise) {
      c_callbacks.on_client_unadvertise =
        [](const void* context, uint32_t client_id, const foxglove_channel_descriptor* channel) {
          try {
            auto cpp_channel = ChannelDescriptor(channel);
            (static_cast<const RemoteAccessGatewayCallbacks*>(context))
              ->onClientUnadvertise(client_id, cpp_channel);
          } catch (const std::exception& exc) {
            warn() << "onClientUnadvertise callback failed: " << exc.what();
          }
        };
    }
  }

  // Build C options
  foxglove_gateway_options c_options = {};
  c_options.context = options.context.getInner();
  c_options.name = {options.name.c_str(), options.name.length()};
  c_options.device_token = {options.device_token.c_str(), options.device_token.length()};
  c_options.callbacks = has_any_callbacks ? &c_callbacks : nullptr;
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
  if (options.sink_channel_filter) {
    sink_channel_filter = std::make_unique<SinkChannelFilterFn>(options.sink_channel_filter);

    c_options.sink_channel_filter_context = sink_channel_filter.get();
    c_options.sink_channel_filter =
      [](const void* context, const struct foxglove_channel_descriptor* channel) -> bool {
      try {
        if (!context) {
          return true;
        }
        const auto* filter_func = static_cast<const SinkChannelFilterFn*>(context);
        auto cpp_channel = ChannelDescriptor(channel);
        return (*filter_func)(cpp_channel);
      } catch (const std::exception& exc) {
        warn() << "Sink channel filter failed: " << exc.what();
        return false;
      }
    };
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

  // Optional backlog size
  if (options.message_backlog_size) {
    c_options.message_backlog_size = &*options.message_backlog_size;
  }

  foxglove_gateway* gateway = nullptr;
  foxglove_error error = foxglove_gateway_start(&c_options, &gateway);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || gateway == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return RemoteAccessGateway(gateway, std::move(callbacks), std::move(sink_channel_filter));
}

RemoteAccessGateway::RemoteAccessGateway(
  foxglove_gateway* gateway, std::unique_ptr<RemoteAccessGatewayCallbacks> callbacks,
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter
)
    : callbacks_(std::move(callbacks))
    , sink_channel_filter_(std::move(sink_channel_filter))
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

FoxgloveError RemoteAccessGateway::stop() {
  foxglove_error error = foxglove_gateway_stop(impl_.release());
  return FoxgloveError(error);
}

}  // namespace foxglove
