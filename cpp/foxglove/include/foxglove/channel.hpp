#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/schemas.hpp>

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;
struct foxglove_context;

/// The foxglove namespace.
namespace foxglove {

/// @brief A description of the data format of messages in a channel.
///
/// It allows Foxglove to validate messages and provide richer visualizations.
struct Schema {
  /// @brief An identifier for the schema.
  std::string name;
  /// @brief The encoding of the schema data. For example "jsonschema" or "protobuf".
  ///
  /// The [well-known schema encodings] are preferred.
  ///
  /// [well-known schema encodings]: https://mcap.dev/spec/registry#well-known-schema-encodings
  std::string encoding;
  /// @brief Must conform to the schema encoding. If encoding is an empty string, data should be 0
  /// length.
  const std::byte* data = nullptr;
  /// @brief The length of the schema data.
  size_t data_len = 0;
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
  static FoxgloveResult<RawChannel> create(
    const std::string_view& topic, const std::string_view& message_encoding,
    std::optional<Schema> schema = std::nullopt, const Context& context = Context()
  );

  /// @brief Log a message to the channel.
  ///
  /// @note Logging is thread-safe. The data will be logged atomically
  /// before or after data logged from other threads.
  ///
  /// @param data The message data.
  /// @param data_len The length of the message data, in bytes.
  /// @param log_time The timestamp of the message. If omitted, the current time is used.
  FoxgloveError log(
    const std::byte* data, size_t data_len, std::optional<uint64_t> log_time = std::nullopt
  ) noexcept;

  /// @brief Uniquely identifies a channel in the context of this program.
  ///
  /// @return The ID of the channel.
  [[nodiscard]] uint64_t id() const noexcept;

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
