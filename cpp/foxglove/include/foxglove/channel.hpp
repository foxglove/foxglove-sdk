#pragma once

#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/messages.hpp>
#include <foxglove/schema.hpp>

#include <cstdint>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <type_traits>

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
  [[nodiscard]] std::string_view topic() const noexcept;

  /// @brief Get the message encoding of the channel descriptor.
  [[nodiscard]] std::string_view messageEncoding() const noexcept;

  /// @deprecated Use messageEncoding() instead.
  // NOLINTNEXTLINE(readability-identifier-naming)
  [[deprecated("Use messageEncoding() instead")]] [[nodiscard]] std::string_view message_encoding(
  ) const noexcept {
    return messageEncoding();
  }

  /// @brief Get the metadata for the channel descriptor.
  [[nodiscard]] std::optional<std::map<std::string, std::string>> metadata() const noexcept;

  /// @brief Get the schema of the channel descriptor.
  [[nodiscard]] std::optional<Schema> schema() const noexcept;
};

/// @brief A function that can be used to filter channels.
///
/// Accepts any callable with signature `bool(const ChannelDescriptor&)`.
/// Callables using the previous `bool(ChannelDescriptor&&)` signature are also accepted but
/// deprecated.
///
/// @return false if the channel should not be logged to the given sink. By default, all channels
/// are logged to a sink.
class SinkChannelFilterFn {
public:
  SinkChannelFilterFn() = default;

  /// @brief Construct from a callable that takes `const ChannelDescriptor&`.
  template<
    typename F, typename = std::enable_if_t<
                  std::is_invocable_r_v<bool, F, const ChannelDescriptor&> &&
                  !std::is_same_v<std::decay_t<F>, SinkChannelFilterFn>>>
  // NOLINTNEXTLINE(google-explicit-constructor,hicpp-explicit-conversions)
  SinkChannelFilterFn(F&& fn)
      : fn_(std::forward<F>(fn)) {}

  /// @deprecated Use a filter function taking `const ChannelDescriptor&` instead of
  /// `ChannelDescriptor&&`.
  template<
    typename F,
    typename = std::enable_if_t<
      std::is_invocable_r_v<bool, F, ChannelDescriptor&&> &&
      !std::is_invocable_v<F, const ChannelDescriptor&> &&
      !std::is_same_v<std::decay_t<F>, SinkChannelFilterFn>>,
    typename /*Disambiguate*/ = void>
  [[deprecated(
    "Use a filter function taking const ChannelDescriptor& instead of ChannelDescriptor&&"
  )]]
  // NOLINTNEXTLINE(google-explicit-constructor,hicpp-explicit-conversions)
  SinkChannelFilterFn(F&& fn)
      : fn_([f = std::forward<F>(fn)](const ChannelDescriptor& ch) mutable {
        auto copy = ch;
        return f(std::move(copy));
      }) {}

  /// @brief Check if a filter function has been set.
  explicit operator bool() const {
    return static_cast<bool>(fn_);
  }

  /// @brief Invoke the filter function.
  bool operator()(const ChannelDescriptor& channel) const {
    return fn_(channel);
  }

private:
  std::function<bool(const ChannelDescriptor&)> fn_;
};

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
  [[nodiscard]] std::string_view messageEncoding() const noexcept;

  /// @deprecated Use messageEncoding() instead.
  // NOLINTNEXTLINE(readability-identifier-naming)
  [[deprecated("Use messageEncoding() instead")]] [[nodiscard]] std::string_view message_encoding(
  ) const noexcept {
    return messageEncoding();
  }

  /// @brief Find out if any sinks have been added to the channel.
  ///
  /// @return True if sinks have been added to the channel, false otherwise.
  [[nodiscard]] bool hasSinks() const noexcept;

  /// @deprecated Use hasSinks() instead.
  // NOLINTNEXTLINE(readability-identifier-naming)
  [[deprecated("Use hasSinks() instead")]] [[nodiscard]] bool has_sinks() const noexcept {
    return hasSinks();
  }

  /// @brief Get the schema of the channel.
  ///
  /// @return The schema of the channel. The value is valid only for the lifetime of the channel.
  [[nodiscard]] std::optional<Schema> schema() const noexcept;

  /// @brief Get the metadata for the channel, set during creation.
  ///
  /// @return The metadata, or an empty map if it was not set.
  [[nodiscard]] std::optional<std::map<std::string, std::string>> metadata() const noexcept;

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

  messages::ChannelUniquePtr impl_;
};

}  // namespace foxglove
