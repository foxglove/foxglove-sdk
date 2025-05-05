#pragma once

#include <foxglove/context.hpp>

#include <memory>
#include <string_view>

enum foxglove_error : uint8_t;
enum class FoxgloveError : uint8_t;
struct foxglove_mcap_writer;
struct foxglove_context;

namespace foxglove {

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
  uint64_t chunk_size = static_cast<uint64_t>(1024 * 768);
  McapCompression compression = McapCompression::Zstd;
  bool use_chunks = true;
  bool disable_seeking = false;
  bool emit_statistics = true;
  bool emit_summary_offsets = true;
  bool emit_message_indexes = true;
  bool emit_chunk_indexes = true;
  bool emit_attachment_indexes = true;
  bool emit_metadata_indexes = true;
  bool repeat_channels = true;
  bool repeat_schemas = true;
  bool truncate = false;

  McapWriterOptions() = default;
};

class McapWriter final {
public:
  static FoxgloveResult<McapWriter> create(const McapWriterOptions& options);

  FoxgloveError close();

  // Default destructor & move, disable copy.
  McapWriter(McapWriter&&) = default;
  ~McapWriter() = default;
  McapWriter& operator=(McapWriter&&) = default;
  McapWriter(const McapWriter&) = delete;
  McapWriter& operator=(const McapWriter&) = delete;

private:
  explicit McapWriter(foxglove_mcap_writer* writer);

  std::unique_ptr<foxglove_mcap_writer, foxglove_error (*)(foxglove_mcap_writer*)> impl_;
};

}  // namespace foxglove
