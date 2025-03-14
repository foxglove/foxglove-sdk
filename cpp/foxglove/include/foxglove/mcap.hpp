#pragma once

#include <memory>
#include <string>

struct foxglove_mcap_writer;

namespace foxglove {

struct McapWriterOptions {
  std::string_view path;
  std::string_view profile;
  std::string_view library;
  uint64_t chunkSize;
  bool useChunks;
  bool disableSeeking;
  bool emitStatistics;
  bool emitSummaryOffsets;
  bool emitMessageIndexes;
  bool emitChunkIndexes;
  bool emitAttachmentIndexes;
  bool emitMetadataIndexes;
  bool repeatChannels;
  bool repeatSchemas;
  bool create;
  bool truncate;
};

class McapWriter final {
public:
  explicit McapWriter(McapWriterOptions options);

  void close();
private:
  std::unique_ptr<foxglove_mcap_writer, void (*)(foxglove_mcap_writer*)> _impl;
};

}  // namespace foxglove
