#include <foxglove-c/foxglove-c.h>
#include <foxglove/cloud_sink.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>

#include <cstdint>
#include <optional>
#include <string>

namespace foxglove {

FoxgloveResult<CloudSink> CloudSink::create(
  CloudSinkOptions&& options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  bool has_any_callbacks = options.callbacks.onSubscribe || options.callbacks.onUnsubscribe ||
                           options.callbacks.onClientAdvertise || options.callbacks.onMessageData ||
                           options.callbacks.onClientUnadvertise;

  std::unique_ptr<CloudSinkCallbacks> callbacks;
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter;
  foxglove_cloud_sink_callbacks c_callbacks = {};

  if (has_any_callbacks) {
    callbacks = std::make_unique<CloudSinkCallbacks>(std::move(options.callbacks));
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
          (static_cast<const CloudSinkCallbacks*>(context))
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
            (static_cast<const CloudSinkCallbacks*>(context))
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
            (static_cast<const CloudSinkCallbacks*>(context))
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
          (static_cast<const CloudSinkCallbacks*>(context))
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
            (static_cast<const CloudSinkCallbacks*>(context))
              ->onClientUnadvertise(client_id, client_channel_id);
          } catch (const std::exception& exc) {
            warn() << "onClientUnadvertise callback failed: " << exc.what();
          }
        };
    }
  }

  foxglove_cloud_sink_options c_options = {};
  c_options.context = options.context.getInner();
  c_options.callbacks = has_any_callbacks ? &c_callbacks : nullptr;
  std::vector<foxglove_string> supported_encodings;
  supported_encodings.reserve(options.supported_encodings.size());
  for (const auto& encoding : options.supported_encodings) {
    supported_encodings.push_back({encoding.c_str(), encoding.length()});
  }
  c_options.supported_encodings = supported_encodings.data();
  c_options.supported_encodings_count = supported_encodings.size();

  if (options.sink_channel_filter) {
    sink_channel_filter = std::make_unique<SinkChannelFilterFn>(options.sink_channel_filter);

    c_options.sink_channel_filter_context = sink_channel_filter.get();
    c_options.sink_channel_filter =
      [](const void* context, const struct foxglove_channel_descriptor* channel) -> bool {
      try {
        if (!context) {
          return true;  // Default to allowing if no filter
        }
        auto* filter_func = static_cast<const SinkChannelFilterFn*>(context);
        auto cpp_channel = ChannelDescriptor(channel);
        return (*filter_func)(std::move(cpp_channel));
      } catch (const std::exception& exc) {
        warn() << "Sink channel filter failed: " << exc.what();
        return false;
      }
    };
  }

  foxglove_cloud_sink* sink = nullptr;
  foxglove_error error = foxglove_cloud_sink_start(&c_options, &sink);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || sink == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return CloudSink(sink, std::move(callbacks), std::move(sink_channel_filter));
}

CloudSink::CloudSink(
  foxglove_cloud_sink* sink, std::unique_ptr<CloudSinkCallbacks> callbacks,
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter
)
    : callbacks_(std::move(callbacks))
    , sink_channel_filter_(std::move(sink_channel_filter))
    , impl_(sink, foxglove_cloud_sink_stop) {}

FoxgloveError CloudSink::stop() {
  foxglove_error error = foxglove_cloud_sink_stop(impl_.release());
  return FoxgloveError(error);
}

}  // namespace foxglove
