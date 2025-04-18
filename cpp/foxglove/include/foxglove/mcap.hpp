#pragma once

#include <memory>
#include <optional>
#include <string>

enum foxglove_error : uint8_t;
enum class FoxgloveError : uint8_t;
struct foxglove_mcap_writer;
struct foxglove_context;

namespace foxglove {

struct Context;

enum class McapCompression : uint8_t {
  None,
  Zstd,
  Lz4,
};

struct McapWriterOptions {
  friend class McapWriter;

  Context context;
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
  bool truncate = false;

  McapWriterOptions() = default;
};

class McapWriter final {
public:
  static FoxgloveResult<McapWriter> create(const McapWriterOptions& options);

  FoxgloveError close();
  McapWriter(McapWriter&&) = default;

private:
  explicit McapWriter(foxglove_mcap_writer* writer);

  std::unique_ptr<foxglove_mcap_writer, foxglove_error (*)(foxglove_mcap_writer*)> _impl;
};

}  // namespace foxglove
