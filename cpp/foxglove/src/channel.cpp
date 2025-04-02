#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>

#include "schema_traits.hpp"

namespace foxglove {

RawChannel::RawChannel(
  const std::string& topic, const std::string& messageEncoding, std::optional<Schema> schema
)
    : _impl(nullptr, foxglove_raw_channel_free) {
  foxglove_schema cSchema = {};
  if (schema) {
    cSchema.name = schema->name.data();
    cSchema.encoding = schema->encoding.data();
    cSchema.data = reinterpret_cast<const uint8_t*>(schema->data);
    cSchema.data_len = schema->dataLen;
  }
  _impl.reset(
    foxglove_raw_channel_create(topic.data(), messageEncoding.data(), schema ? &cSchema : nullptr)
  );
}

uint64_t RawChannel::id() const {
  return foxglove_raw_channel_get_id(_impl.get());
}

void RawChannel::log(
  const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime,
  std::optional<uint64_t> publishTime, std::optional<uint32_t> sequence
) {
  foxglove_raw_channel_log(
    _impl.get(),
    reinterpret_cast<const uint8_t*>(data),
    dataLen,
    logTime ? &*logTime : nullptr,
    publishTime ? &*publishTime : nullptr,
    sequence ? &*sequence : nullptr
  );
}

template<class TMsg>
Channel<TMsg, std::enable_if_t<foxglove::internal::IsBuiltinSchema<TMsg>::value>>::Channel(
  const std::string& topic
)
    : _impl(
        foxglove_channel_create(topic.data(), internal::BuiltinSchemaTraits<TMsg>::BuiltinSchema),
        foxglove_channel_free
      ) {}

template<class TMsg>
uint64_t Channel<TMsg, std::enable_if_t<foxglove::internal::IsBuiltinSchema<TMsg>::value>>::id(
) const {
  return foxglove_channel_get_id(_impl.get());
}

template<class TMsg>
void Channel<TMsg, std::enable_if_t<foxglove::internal::IsBuiltinSchema<TMsg>::value>>::log(
  const TMsg& msg, std::optional<uint64_t> logTime, std::optional<uint64_t> publishTime,
  std::optional<uint32_t> sequence
) {
  internal::BuiltinSchemaTraits<TMsg>::WithCMessage(
    msg,
    [&](const typename internal::BuiltinSchemaTraits<TMsg>::CType& cMsg) {
      foxglove_channel_log(
        _impl.get(),
        &cMsg,
        logTime ? &*logTime : nullptr,
        publishTime ? &*publishTime : nullptr,
        sequence ? &*sequence : nullptr
      );
    }
  );
}

template class Channel<foxglove::schemas::Vector3>;

}  // namespace foxglove
