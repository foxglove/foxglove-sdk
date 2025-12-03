#include <foxglove-c/foxglove-c.h>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>

namespace foxglove {

static int custom_flush(void* fn) {
  auto* writer = static_cast<CustomWriter*>(fn);
  return writer->flush();
}

static int custom_seek(void* fn, int64_t pos, int whence, uint64_t* new_pos) {
  auto* writer = static_cast<CustomWriter*>(fn);
  return writer->seek(pos, whence, new_pos);
}

static size_t custom_write(void* fn, const uint8_t* data, size_t len, int32_t* error) {
  auto* writer = static_cast<CustomWriter*>(fn);
  return writer->write(data, len, error);
}

FoxgloveResult<McapWriter> McapWriter::create(const McapWriterOptions& options) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_mcap_options c_options = {};
  c_options.context = options.context.getInner();
  c_options.path = {options.path.data(), options.path.length()};
  c_options.profile = {options.profile.data(), options.profile.length()};

  // Handle custom writer if provided
  std::unique_ptr<CustomWriter> custom_writer;
  foxglove_custom_writer c_custom_writer;
  if (options.custom_writer.has_value()) {
    custom_writer = std::make_unique<CustomWriter>(options.custom_writer.value());
    c_custom_writer.context = custom_writer.get();
    c_custom_writer.write_fn = custom_write;
    c_custom_writer.flush_fn = custom_flush;
    c_custom_writer.seek_fn = custom_seek;
    c_options.custom_writer = &c_custom_writer;
  } else {
    c_options.custom_writer = nullptr;
  }

  // TODO FG-11215: generate the enum for C++ from the C enum
  // so this is guaranteed to never get out of sync
  c_options.compression = static_cast<foxglove_mcap_compression>(options.compression);
  c_options.chunk_size = options.chunk_size;
  c_options.use_chunks = options.use_chunks;
  c_options.disable_seeking = options.disable_seeking;
  c_options.emit_statistics = options.emit_statistics;
  c_options.emit_summary_offsets = options.emit_summary_offsets;
  c_options.emit_message_indexes = options.emit_message_indexes;
  c_options.emit_chunk_indexes = options.emit_chunk_indexes;
  c_options.emit_attachment_indexes = options.emit_attachment_indexes;
  c_options.emit_metadata_indexes = options.emit_metadata_indexes;
  c_options.repeat_channels = options.repeat_channels;
  c_options.repeat_schemas = options.repeat_schemas;
  c_options.truncate = options.truncate;

  // Handle sink channel filter with context
  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter;
  if (options.sink_channel_filter) {
    // Create a wrapper to hold the function
    sink_channel_filter = std::make_unique<SinkChannelFilterFn>(options.sink_channel_filter);

    c_options.sink_channel_filter_context = sink_channel_filter.get();
    c_options.sink_channel_filter =
      [](const void* context, const struct foxglove_channel_descriptor* channel) -> bool {
      try {
        if (!context) {
          return true;
        }
        auto* filter_func = static_cast<const SinkChannelFilterFn*>(context);
        auto cpp_channel = ChannelDescriptor(channel);
        return (*filter_func)(std::move(cpp_channel));
      } catch (const std::exception& exc) {
        warn() << "Sink channel filter failed: " << exc.what();
        return false;
      }
    };
  }

  foxglove_mcap_writer* writer = nullptr;
  foxglove_error error = foxglove_mcap_open(&c_options, &writer);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || writer == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return McapWriter(writer, std::move(sink_channel_filter), std::move(custom_writer));
}

McapWriter::McapWriter(
  foxglove_mcap_writer* writer, std::unique_ptr<SinkChannelFilterFn> sink_channel_filter,
  std::unique_ptr<CustomWriter> custom_writer
)
    : sink_channel_filter_(std::move(sink_channel_filter))
    , custom_writer_(std::move(custom_writer))
    , impl_(writer, foxglove_mcap_close) {}

FoxgloveError McapWriter::close() {
  foxglove_error error = foxglove_mcap_close(impl_.release());
  return FoxgloveError(error);
}

}  // namespace foxglove
