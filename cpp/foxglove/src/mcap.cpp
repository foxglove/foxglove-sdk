#include <foxglove-c/foxglove-c.h>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>

namespace foxglove {

FoxgloveResult<McapWriter> McapWriter::create(const McapWriterOptions& options) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_mcap_options c_options = {};
  c_options.context = options.context.getInner();
  c_options.path = {options.path.data(), options.path.length()};
  c_options.profile = {options.profile.data(), options.profile.length()};
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

  foxglove_mcap_writer* writer = nullptr;
  foxglove_error error = foxglove_mcap_open(&c_options, &writer);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || writer == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return McapWriter(writer);
}

McapWriter::McapWriter(foxglove_mcap_writer* writer)
    : impl_(writer, foxglove_mcap_close) {}

FoxgloveError McapWriter::close() {
  foxglove_error error = foxglove_mcap_close(impl_.release());
  return FoxgloveError(error);
}

}  // namespace foxglove
