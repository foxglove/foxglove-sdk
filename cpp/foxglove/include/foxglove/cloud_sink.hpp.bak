#pragma once

#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>

struct foxglove_cloud_sink;

namespace foxglove {

/// @brief The callback interface for a Cloud Sink.
///
/// These methods are invoked from the client's main poll loop and must be as low-latency as
/// possible.
///
/// @note These callbacks may be invoked concurrently from multiple threads.
/// You must synchronize access to your mutable internal state or shared resources.
struct CloudSinkCallbacks {
  /// @brief Callback invoked when a client subscribes to a channel.
  ///
  /// Only invoked if the channel is associated with the cloud sink and isn't already subscribed to
  /// by the client.
  std::function<void(uint64_t channel_id, const ClientMetadata& client_metadata)> onSubscribe;

  /// @brief Callback invoked when a client unsubscribes from a channel.
  ///
  /// Only invoked for channels that had an active subscription from the client.
  std::function<void(uint64_t channel_id, const ClientMetadata& client_metadata)> onUnsubscribe;

  /// @brief Callback invoked when a client advertises a client channel.
  std::function<void(uint32_t client_id, const ClientChannel& channel)> onClientAdvertise;

  /// @brief Callback invoked when a client message is received
  std::function<
    void(uint32_t client_id, uint32_t client_channel_id, const std::byte* data, size_t data_len)>
    onMessageData;

  /// @brief Callback invoked when a client unadvertises a client channel.
  std::function<void(uint32_t client_id, uint32_t client_channel_id)> onClientUnadvertise;
};

/// @brief Options for a Cloud Sink.
struct CloudSinkOptions {
  friend class CloudSink;

  /// @brief The logging context for this cloud sink.
  Context context;
  /// @brief The callbacks of the cloud sink.
  CloudSinkCallbacks callbacks;
  /// @brief The supported encodings of the cloud sink.
  std::vector<std::string> supported_encodings;
  /// @brief A sink channel filter callback.
  SinkChannelFilterFn sink_channel_filter;
};

/// @brief A Cloud Sink for visualization in Foxglove.
///
/// Requires Foxglove Agent to be running on the same machine.
///
/// @note CloudSink is fully thread-safe, but CloudSinkCallbacks may be invoked
/// concurrently from multiple threads, so you will need to use synchronization in your callbacks.
class CloudSink final {
public:
  /// @brief Create a new CloudSink cloud sink with the given options.
  static FoxgloveResult<CloudSink> create(CloudSinkOptions&& options);

  /// @brief Gracefully shut down the cloud sink connection.
  FoxgloveError stop();

private:
  CloudSink(
    foxglove_cloud_sink* sink, std::unique_ptr<CloudSinkCallbacks> callbacks,
    std::unique_ptr<SinkChannelFilterFn> sink_channel_filter
  );

  std::unique_ptr<CloudSinkCallbacks> callbacks_;
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter_;
  std::unique_ptr<foxglove_cloud_sink, foxglove_error (*)(foxglove_cloud_sink*)> impl_;
};

}  // namespace foxglove
