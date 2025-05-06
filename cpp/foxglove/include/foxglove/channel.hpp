#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;
struct foxglove_context;

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
  /// @brief Must conform to the schema encoding. If encoding is an empty string, data should be 0 length.
  const std::byte* data = nullptr;
  /// @brief The length of the schema data.
  size_t data_len = 0;
};

/// @brief A channel for messages logged to a topic.
class Channel final {
public:
  /// @brief Create a new channel.
  ///
  /// @param topic The topic name. You should choose a unique topic name per channel for compatibility with the Foxglove app.
  /// @param message_encoding The encoding of messages logged to this channel.
  /// @param schema The schema of messages logged to this channel.
  /// @param context The context which associates logs to a sink. If omitted, the default context is used.
  static FoxgloveResult<Channel> create(
    const std::string& topic, const std::string& message_encoding,
    std::optional<Schema> schema = std::nullopt, const Context& context = Context()
  );


  /// @brief Log a message to the channel.
  ///
  /// @param data The message data.
  /// @param data_len The length of the message data, in bytes.
  /// @param log_time The timestamp of the message. If omitted, the current time is used.
  FoxgloveError log(
    const std::byte* data, size_t data_len, std::optional<uint64_t> log_time = std::nullopt
  );

  /// @cond test-utilities
  [[nodiscard]] uint64_t testId() const;
  /// @endcond

  /// @brief Uniquely identifies a channel in the context of this program.
  ///
  /// @return The ID of the channel.
  [[nodiscard]] uint64_t id() const;

  Channel(const Channel&) = delete;
  Channel& operator=(const Channel&) = delete;

  /// @brief Default move constructor.
  Channel(Channel&& other) noexcept = default;
  /// @brief Default move assignment.
  Channel& operator=(Channel&& other) noexcept = default;
  /// @brief Default estructor.
  ~Channel() = default;

private:
  explicit Channel(const foxglove_channel* channel);

  std::unique_ptr<const foxglove_channel, void (*const)(const foxglove_channel*)> impl_;
};

}  // namespace foxglove
