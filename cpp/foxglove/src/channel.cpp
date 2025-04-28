#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

namespace foxglove {

FoxgloveResult<Channel> Channel::create(
  const std::string& topic, const std::string& message_encoding, std::optional<Schema> schema,
  const Context& context
) {
  foxglove_schema c_schema = {};
  if (schema) {
    c_schema.name = {schema->name.data(), schema->name.length()};
    c_schema.encoding = {schema->encoding.data(), schema->encoding.length()};
    c_schema.data = reinterpret_cast<const uint8_t*>(schema->data);
    c_schema.data_len = schema->data_len;
  }
  const foxglove_channel* channel = nullptr;
  foxglove_error error = foxglove_channel_create(
    {topic.data(), topic.length()}, {message_encoding.data(), message_encoding.length()},
    schema ? &c_schema : nullptr, context.get_inner(), &channel
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || channel == nullptr) {
    return foxglove::unexpected(FoxgloveError(error));
  }
  return Channel(channel);
}

Channel::Channel(const foxglove_channel* channel)
    : _impl(channel, foxglove_channel_free) {}

uint64_t Channel::id() const {
  return foxglove_channel_get_id(_impl.get());
}

uint64_t Channel::test_ID() const {
  return foxglove_channel_get_id(_impl.get());
}

FoxgloveError Channel::log(
  const std::byte* data, size_t data_len, std::optional<uint64_t> log_time
) {
  foxglove_error error = foxglove_channel_log(
    _impl.get(), reinterpret_cast<const uint8_t*>(data), data_len, log_time ? &*log_time : nullptr
  );
  return FoxgloveError(error);
}

}  // namespace foxglove
