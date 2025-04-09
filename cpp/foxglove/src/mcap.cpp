#include <foxglove-c/foxglove-c.h>
#include <foxglove/mcap.hpp>

namespace foxglove {

FoxgloveMcapCompression toFoxgloveMcapCompression(McapCompression compression) {
  switch (compression) {
    case McapCompression::Zstd:
      return FoxgloveMcapCompression_Zstd;
    case McapCompression::Lz4:
      return FoxgloveMcapCompression_Lz4;
    default:
      return FoxgloveMcapCompression_None;
  }
}

McapWriter::McapWriter(McapWriterOptions options, std::optional<const Context&> context)
    : _impl(nullptr, foxglove_mcap_free) {
  foxglove_internal_register_cpp_wrapper();

  foxglove_mcap_options cOptions = {};
  cOptions.context = context.has_value() ? context->get_inner() : nullptr;
  cOptions.path = options.path.data();
  cOptions.path_len = options.path.length();
  cOptions.profile = options.profile.data();
  cOptions.profile_len = options.profile.length();
  cOptions.compression = toFoxgloveMcapCompression(options.compression);
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
  cOptions.create = options.create;
  cOptions.truncate = options.truncate;
  _impl.reset(foxglove_mcap_open(&cOptions));
}

void McapWriter::close() {
  foxglove_mcap_close(_impl.get());
}

}  // namespace foxglove
