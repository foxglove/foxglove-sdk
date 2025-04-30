#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

namespace foxglove {

FoxgloveResult<RawChannel> RawChannel::create(
  const std::string& topic, const std::string& messageEncoding, std::optional<Schema> schema,
  const Context& context
) {
  foxglove_schema cSchema = {};
  if (schema) {
    cSchema.name = {schema->name.data(), schema->name.length()};
    cSchema.encoding = {schema->encoding.data(), schema->encoding.length()};
    cSchema.data = reinterpret_cast<const uint8_t*>(schema->data);
    cSchema.data_len = schema->dataLen;
  }
  const foxglove_channel* channel = nullptr;
  foxglove_error error = foxglove_channel_create(
    {topic.data(), topic.length()},
    {messageEncoding.data(), messageEncoding.length()},
    schema ? &cSchema : nullptr,
    context.get_inner(),
    &channel
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || channel == nullptr) {
    return foxglove::unexpected(FoxgloveError(error));
  }
  return RawChannel(channel);
}

RawChannel::RawChannel(const foxglove_channel* channel)
    : _impl(channel, foxglove_channel_free) {}

uint64_t RawChannel::id() const {
  return foxglove_channel_get_id(_impl.get());
}

FoxgloveError RawChannel::log(
  const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime
) {
  foxglove_error error = foxglove_channel_log(
    _impl.get(), reinterpret_cast<const uint8_t*>(data), dataLen, logTime ? &*logTime : nullptr
  );
  return FoxgloveError(error);
}

}  // namespace foxglove
