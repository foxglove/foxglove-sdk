#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

namespace foxglove {

FoxgloveResult<RawChannel> RawChannel::create(
  const std::string_view& topic, const std::string_view& message_encoding,
  std::optional<Schema> schema, const Context& context
) {
  foxglove_schema c_schema = {};
  if (schema) {
    c_schema.name = {schema->name.data(), schema->name.length()};
    c_schema.encoding = {schema->encoding.data(), schema->encoding.length()};
    c_schema.data = reinterpret_cast<const uint8_t*>(schema->data);
    c_schema.data_len = schema->data_len;
  }
  const foxglove_channel* channel = nullptr;
  foxglove_error error = foxglove_raw_channel_create(
    {topic.data(), topic.length()},
    {message_encoding.data(), message_encoding.length()},
    schema ? &c_schema : nullptr,
    context.getInner(),
    &channel
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || channel == nullptr) {
    return foxglove::unexpected(FoxgloveError(error));
  }
  return RawChannel(channel);
}

RawChannel::RawChannel(const foxglove_channel* channel)
    : impl_(channel) {}

uint64_t RawChannel::id() const {
  return foxglove_channel_get_id(impl_.get());
}

FoxgloveError RawChannel::log(
  const std::byte* data, size_t data_len, std::optional<uint64_t> log_time
) {
  foxglove_error error = foxglove_channel_log(
    impl_.get(), reinterpret_cast<const uint8_t*>(data), data_len, log_time ? &*log_time : nullptr
  );
  return FoxgloveError(error);
}

}  // namespace foxglove
