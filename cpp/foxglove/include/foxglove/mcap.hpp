#pragma once

#include <memory>
#include <optional>
#include <string>

struct foxglove_mcap_writer;
struct foxglove_context;

namespace foxglove {

struct Context;
typedef foxglove_context ContextInner;

enum class McapCompression {
  None,
  Zstd,
  Lz4,
};

struct McapWriterOptions {
  friend class McapWriter;

  std::string_view path;
  std::string_view profile;
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

  McapWriterOptions() = default;
  explicit McapWriterOptions(const Context& context);

private:
  const ContextInner* context = nullptr;
};

class McapWriter final {
public:
  explicit McapWriter(McapWriterOptions options);

  void close();

private:
  std::unique_ptr<foxglove_mcap_writer, void (*)(foxglove_mcap_writer*)> _impl;
};

}  // namespace foxglove
