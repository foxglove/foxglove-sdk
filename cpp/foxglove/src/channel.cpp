#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>

namespace foxglove {

Channel::Channel(
  const std::string& topic, const std::string& messageEncoding, std::optional<Schema> schema
)
    : _impl(nullptr, foxglove_channel_free) {
  foxglove_schema cSchema = {};
  std::optional<foxglove_schema*> schemaPtr = std::nullopt;

  if (schema) {
    cSchema.name = schema->name.data();
    cSchema.encoding = schema->encoding.data();

    if (schema->data && schema->dataLen > 0) {
      _storedSchema.assign(
        reinterpret_cast<const uint8_t*>(schema->data),
        reinterpret_cast<const uint8_t*>(schema->data) + schema->dataLen
      );
      cSchema.data = _storedSchema.data();
      cSchema.data_len = _storedSchema.size();
    }

    schemaPtr = &cSchema;
  }
  _impl.reset(
    foxglove_channel_create(topic.c_str(), messageEncoding.c_str(), schemaPtr.value_or(nullptr))
  );
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
