#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>

namespace foxglove {

Channel::Channel(
  const std::string& topic, const std::string& messageEncoding, Schema schema,
  const Context& context
)
    : Channel(topic, messageEncoding, schema, context.get_inner()) {}

Channel::Channel(
  const std::string& topic, const std::string& messageEncoding, std::optional<Schema> schema,
  const ContextInner* context
)
    : _impl(nullptr, foxglove_channel_free) {
  foxglove_schema cSchema = {};
  if (schema) {
    cSchema.name = schema->name.data();
    cSchema.encoding = schema->encoding.data();
    cSchema.data = reinterpret_cast<const uint8_t*>(schema->data);
    cSchema.data_len = schema->dataLen;
  }
  _impl.reset(foxglove_channel_create(
    topic.data(), messageEncoding.data(), schema ? &cSchema : nullptr, context
  ));
}

uint64_t Channel::id() const {
  return foxglove_channel_get_id(_impl.get());
}

void Channel::log(
  const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime,
  std::optional<uint64_t> publishTime, std::optional<uint32_t> sequence
) {
  foxglove_channel_log(
    _impl.get(),
    reinterpret_cast<const uint8_t*>(data),
    dataLen,
    logTime ? &*logTime : nullptr,
    publishTime ? &*publishTime : nullptr,
    sequence ? &*sequence : nullptr
  );
}

}  // namespace foxglove
