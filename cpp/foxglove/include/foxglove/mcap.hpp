#pragma once

#include <memory>
#include <string>

#include "foxglove-c/foxglove-c.h"

struct foxglove_mcap_writer;

namespace foxglove {

struct McapWriterOptions {
  std::string_view path;
  std::string_view profile;
  uint64_t chunkSize = 0;
  FoxgloveMcapCompression compression = FoxgloveMcapCompression_None;
  bool useChunks = false;
  bool disableSeeking = false;
  bool emitStatistics = false;
  bool emitSummaryOffsets = false;
  bool emitMessageIndexes = false;
  bool emitChunkIndexes = false;
  bool emitAttachmentIndexes = false;
  bool emitMetadataIndexes = false;
  bool repeatChannels = false;
  bool repeatSchemas = false;
  bool create = false;
  bool truncate = false;
};

class McapWriter final {
public:
  explicit McapWriter(McapWriterOptions options);

  void close();

private:
  std::unique_ptr<foxglove_mcap_writer, void (*)(foxglove_mcap_writer*)> _impl;
};

}  // namespace foxglove
