#pragma once

#include <memory>
#include <optional>
#include <string>

struct foxglove_mcap_writer;

namespace foxglove {

struct Context;

enum class McapCompression {
  None,
  Zstd,
  Lz4,
};

struct McapWriterOptions {
  std::string_view path;
  std::string_view profile;
  std::optional<const Context&> context = std::nullopt;
  uint64_t chunkSize = 1024 * 768;
  McapCompression compression = McapCompression::Zstd;
  bool useChunks = true;
  bool disableSeeking = false;
  bool emitStatistics = true;
  bool emitSummaryOffsets = true;
  bool emitMessageIndexes = true;
  bool emitChunkIndexes = true;
  bool emitAttachmentIndexes = true;
  bool emitMetadataIndexes = true;
  bool repeatChannels = true;
  bool repeatSchemas = true;
  bool create = true;
  bool truncate = false;
};

class McapWriter final {
public:
  McapWriter(McapWriterOptions options, std::optional<const Context&> context = std::nullopt);

  void close();

private:
  std::unique_ptr<foxglove_mcap_writer, void (*)(foxglove_mcap_writer*)> _impl;
};

}  // namespace foxglove
