#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>

namespace foxglove {

Channel::Channel(std::string topic, std::string messageEncoding, std::optional<Schema> schema)
    : _impl(
        foxglove_channel_create(
          topic.c_str(), messageEncoding.c_str(),
          schema ? &((const foxglove_schema&)foxglove_schema{
                     schema->name.data(),
                     schema->encoding.data(),
                     reinterpret_cast<const uint8_t*>(schema->data),
                     schema->dataLen,
                   })
                 : nullptr
        ),
        foxglove_channel_free
      ) {}

void Channel::log(
  const std::byte* data, size_t dataLen, uint64_t logTime, uint64_t publishTime, uint32_t sequence
) {
  foxglove_channel_log(
    _impl.get(), reinterpret_cast<const uint8_t*>(data), dataLen, logTime, publishTime, sequence
  );
}

}  // namespace foxglove
