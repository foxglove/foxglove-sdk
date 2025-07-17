#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

namespace foxglove {

FoxgloveResult<RawChannel> RawChannel::create(
  const std::string_view& topic, const std::string_view& message_encoding,
  std::optional<Schema> schema, const Context& context,
  std::optional<std::map<std::string, std::string>> metadata
) {
  foxglove_schema c_schema = {};
  if (schema) {
    c_schema.name = {schema->name.data(), schema->name.length()};
    c_schema.encoding = {schema->encoding.data(), schema->encoding.length()};
    c_schema.data = reinterpret_cast<const uint8_t*>(schema->data);
    c_schema.data_len = schema->data_len;
  }
  foxglove_channel_metadata c_metadata = {};
  std::vector<foxglove_key_value> metadata_items;  // Keep this alive for the duration of the call
  if (metadata) {
    metadata_items.reserve(metadata->size());
    for (const auto& [key, value] : *metadata) {
      foxglove_string k = {key.data(), key.length()};
      foxglove_string v = {value.data(), value.length()};
      metadata_items.push_back({k, v});
    }
    c_metadata.items = metadata_items.data();
    c_metadata.count = metadata_items.size();
  }

  const foxglove_channel* channel = nullptr;
  foxglove_error error = foxglove_raw_channel_create(
    {topic.data(), topic.length()},
    {message_encoding.data(), message_encoding.length()},
    schema ? &c_schema : nullptr,
    context.getInner(),
    metadata ? &c_metadata : nullptr,
    &channel
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || channel == nullptr) {
    return tl::unexpected(FoxgloveError(error));
  }
  return RawChannel(channel);
}

RawChannel::RawChannel(const foxglove_channel* channel)
    : impl_(channel) {}

void RawChannel::close() noexcept {
  foxglove_channel_close(impl_.get());
}

uint64_t RawChannel::id() const noexcept {
  return foxglove_channel_get_id(impl_.get());
}

std::string_view RawChannel::topic() const noexcept {
  foxglove_string string = foxglove_channel_get_topic(impl_.get());
  return std::string_view(string.data, string.len);
}

std::string_view RawChannel::message_encoding() const noexcept {
  foxglove_string string = foxglove_channel_get_message_encoding(impl_.get());
  return std::string_view(string.data, string.len);
}

bool RawChannel::has_sinks() const noexcept {
  return foxglove_channel_has_sinks(impl_.get());
}

std::optional<Schema> RawChannel::schema() const noexcept {
  foxglove_schema c_schema = {};
  foxglove_error error = foxglove_channel_get_schema(impl_.get(), &c_schema);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    return std::nullopt;
  }

  Schema schema;
  schema.name = std::string(c_schema.name.data, c_schema.name.len);
  schema.encoding = std::string(c_schema.encoding.data, c_schema.encoding.len);
  schema.data = reinterpret_cast<const std::byte*>(c_schema.data);
  schema.data_len = c_schema.data_len;
  return schema;
}

std::optional<std::map<std::string, std::string>> RawChannel::metadata() const noexcept {
  std::map<std::string, std::string> result;

  foxglove_channel_metadata_iterator* iter = foxglove_channel_metadata_iter_create(impl_.get());
  if (!iter) {
    return std::nullopt;
  }

  struct foxglove_key_value item;
  while (foxglove_channel_metadata_iter_next(iter, &item)) {
    result[std::string(item.key.data, item.key.len)] = std::string(item.value.data, item.value.len);
  }

  foxglove_channel_metadata_iter_free(iter);

  return result;
}

FoxgloveError RawChannel::log(
  const std::byte* data, size_t data_len, std::optional<uint64_t> log_time
) noexcept {
  return log_(data, data_len, log_time, std::nullopt);
}

/// @cond foxglove_internal
FoxgloveError RawChannel::log_(
  const std::byte* data, size_t data_len, std::optional<uint64_t> log_time,
  std::optional<uint64_t> sink_id
) noexcept {
  foxglove_error error = foxglove_channel_log(
    impl_.get(),
    reinterpret_cast<const uint8_t*>(data),
    data_len,
    log_time ? &*log_time : nullptr,
    sink_id ? *sink_id : 0
  );
  return FoxgloveError(error);
}
/// @endcond

}  // namespace foxglove
