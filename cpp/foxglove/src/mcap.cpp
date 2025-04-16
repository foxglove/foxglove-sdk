#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>

namespace foxglove {

FoxgloveResult<McapWriter> McapWriter::create(const McapWriterOptions& options) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_mcap_options cOptions = {};
  cOptions.path = options.path.data();
  cOptions.path_len = options.path.length();
  cOptions.profile = options.profile.data();
  cOptions.profile_len = options.profile.length();
  // TODO FG-11215: generate the enum for C++ from the C enum
  // so this is guaranteed to never get out of sync
  cOptions.compression = static_cast<foxglove_mcap_compression>(options.compression);
  cOptions.chunk_size = options.chunkSize;
  cOptions.use_chunks = options.useChunks;
  cOptions.disable_seeking = options.disableSeeking;
  cOptions.emit_statistics = options.emitStatistics;
  cOptions.emit_summary_offsets = options.emitSummaryOffsets;
  cOptions.emit_message_indexes = options.emitMessageIndexes;
  cOptions.emit_chunk_indexes = options.emitChunkIndexes;
  cOptions.emit_attachment_indexes = options.emitAttachmentIndexes;
  cOptions.emit_metadata_indexes = options.emitMetadataIndexes;
  cOptions.repeat_channels = options.repeatChannels;
  cOptions.repeat_schemas = options.repeatSchemas;
  cOptions.truncate = options.truncate;

  foxglove_mcap_writer* writer = nullptr;
  foxglove_error error = foxglove_mcap_open(&cOptions, &writer);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || writer == nullptr) {
    return tl::unexpected(static_cast<FoxgloveError>(error));
  }

  return McapWriter(writer);
}

McapWriter::McapWriter(foxglove_mcap_writer* writer)
    : _impl(writer, foxglove_mcap_close) {}

FoxgloveError McapWriter::close() {
  foxglove_error error = foxglove_mcap_close(_impl.get());
  _impl.reset();
  return FoxgloveError(error);
}

}  // namespace foxglove
