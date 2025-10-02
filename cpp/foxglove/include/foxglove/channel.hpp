#pragma once

#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/schema.hpp>
#include <foxglove/schemas.hpp>

#include <cstdint>
#include <map>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;
struct foxglove_channel_descriptor;

/// The foxglove namespace.
namespace foxglove {

/// @brief A description of a channel. This will be constructed by the SDK and passed to an
/// implementation of a `SinkChannelFilterFn`.
class ChannelDescriptor {
  const foxglove_channel_descriptor* channel_descriptor_;

public:
  /// @cond foxglove_internal
  /// @brief Information about a channel. This is constructed internally.
  explicit ChannelDescriptor(const foxglove_channel_descriptor* channel_descriptor);
  /// @endcond

  /// @brief Get the topic of the channel descriptor.
  [[nodiscard]] const std::string_view topic() const noexcept;

  /// @brief Get the message encoding of the channel descriptor.
  [[nodiscard]] const std::string_view message_encoding() const noexcept;

  /// @brief Get the metadata for the channel descriptor.
  [[nodiscard]] const std::optional<std::map<std::string, std::string>> metadata() const noexcept;

  /// @brief Get the schema of the channel descriptor.
  [[nodiscard]] const std::optional<Schema> schema() const noexcept;
};

/// @brief A function that can be used to filter channels.
///
/// @param channel Information about the channel.
/// @return false if the channel should not be logged to the given sink. By default, all channels
/// are logged to a sink.
using SinkChannelFilterFn = std::function<bool(ChannelDescriptor&& channel)>;

/// @brief A channel for messages logged to a topic.
///
/// @note Channels are fully thread-safe. Creating channels and logging on them
/// is safe from any number of threads concurrently. A channel can be created
/// on one thread and sent to and destroyed on another.
class RawChannel final {
public:
  /// @brief Create a new channel.
  ///
  /// @param topic The topic name. You should choose a unique topic name per channel for
  /// compatibility with the Foxglove app.
  /// @param message_encoding The encoding of messages logged to this channel.
  /// @param schema The schema of messages logged to this channel.
  /// @param context The context which associates logs to a sink. If omitted, the default context is
  /// used.
  /// @param metadata Key/value metadata for the channel.
  static FoxgloveResult<RawChannel> create(
    const std::string_view& topic, const std::string_view& message_encoding,
    std::optional<Schema> schema = std::nullopt, const Context& context = Context(),
    std::optional<std::map<std::string, std::string>> metadata = std::nullopt
  );

  /// @brief Log a message to the channel.
  ///
  /// @note Logging is thread-safe. The data will be logged atomically
  /// before or after data logged from other threads.
  ///
  /// @param data The message data.
  /// @param data_len The length of the message data, in bytes.
  /// @param log_time The timestamp of the message, as nanoseconds since epoch. If omitted, the
  /// current time is used.
  /// @param sink_id The sink ID associated with the message. Can be used to target logging messages
  /// to a specific client or mcap file. If omitted, the message is logged to all sinks. Note that
  /// providing a sink_id is not yet part of the public API. To partition logs among specific sinks,
  /// set up different `Context`s.
  FoxgloveError log(
    const std::byte* data, size_t data_len, std::optional<uint64_t> log_time = std::nullopt,
    std::optional<uint64_t> sink_id = std::nullopt
  ) noexcept;

  /// @brief Close the channel.
  ///
  /// You can use this to explicitly unadvertise the channel to sinks that subscribe to channels
  /// dynamically, such as the WebSocketServer.
  ///
  /// Attempts to log on a closed channel will elicit a throttled warning message.
  void close() noexcept;

  /// @brief Uniquely identifies a channel in the context of this program.
  ///
  /// @return The ID of the channel.
  [[nodiscard]] uint64_t id() const noexcept;

  /// @brief Get the topic of the channel.
  ///
  /// @return The topic of the channel. The value is valid only for the lifetime of the channel.
  [[nodiscard]] std::string_view topic() const noexcept;

  /// @brief Get the message encoding of the channel.
  ///
  /// @return The message encoding of the channel. The value is valid only for the lifetime of the
  /// channel.
  [[nodiscard]] std::string_view message_encoding() const noexcept;

  /// @brief Find out if any sinks have been added to the channel.
  ///
  /// @return True if sinks have been added to the channel, false otherwise.
  [[nodiscard]] bool has_sinks() const noexcept;

  /// @brief Get the schema of the channel.
  ///
  /// @return The schema of the channel. The value is valid only for the lifetime of the channel.
  [[nodiscard]] std::optional<Schema> schema() const noexcept;

  /// @brief Get the metadata for the channel, set during creation.
  ///
  /// @return The metadata, or an empty map if it was not set.
  std::optional<std::map<std::string, std::string>> metadata() const noexcept;

  RawChannel(const RawChannel&) = delete;
  RawChannel& operator=(const RawChannel&) = delete;
  /// @brief Default move constructor.
  RawChannel(RawChannel&& other) noexcept = default;
  /// @brief Default move assignment.
  RawChannel& operator=(RawChannel&& other) noexcept = default;
  /// @brief Default destructor
  ~RawChannel() = default;

private:
  explicit RawChannel(const foxglove_channel* channel);

  schemas::ChannelUniquePtr impl_;
};

}  // namespace foxglove
