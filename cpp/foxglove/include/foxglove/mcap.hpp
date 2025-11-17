#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

#include <functional>
#include <memory>
#include <optional>
#include <string>

enum foxglove_error : uint8_t;
struct foxglove_mcap_writer;
struct FoxgloveCustomWriter;

/// The foxglove namespace.
namespace foxglove {

class Context;

/// @brief Custom writer for writing MCAP data to arbitrary destinations.
///
/// This provides a simple function pointer interface that matches the C API.
/// Users are responsible for managing the lifetime of user_data and ensuring
/// thread safety if needed.
struct CustomWriter {
  /// @brief User-provided context pointer, passed to all callback functions
  void* user_data = nullptr;

  /// @brief Write function: write data to the custom destination
  /// @param user_data User-provided context pointer
  /// @param data Pointer to data to write
  /// @param len Number of bytes to write
  /// @param error Pointer to error code (set to an error number defined in errno.h if write fails)
  /// @return Number of bytes actually written
  std::function<size_t(void* user_data, const uint8_t* data, size_t len, int32_t* error)> write_fn;

  /// @brief Flush function: ensure all buffered data is written
  /// @param user_data User-provided context pointer
  /// @return 0 on success, an error number defined in errno.h if flush fails
  std::function<int32_t(void* user_data)> flush_fn;

  /// @brief Seek function: change the current position in the stream
  /// @param user_data User-provided context pointer
  /// @param pos Position offset
  /// @param whence Seek origin (0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END)
  /// @param new_pos Pointer to store the new absolute position
  /// @return 0 on success, an error number defined in errno.h if seek fails
  std::function<int32_t(void* user_data, int64_t pos, int32_t whence, uint64_t* new_pos)> seek_fn;
};

/// @brief The compression algorithm to use for an MCAP file.
enum class McapCompression : uint8_t {
  /// No compression.
  None,
  /// Zstd compression.
  Zstd,
  /// LZ4 compression.
  Lz4,
};

/// @brief Options for an MCAP writer.
struct McapWriterOptions {
  friend class McapWriter;

  /// @brief The context to use for the MCAP writer.
  Context context;
  /// @brief The path to the MCAP file. Ignored if custom_writer is set.
  std::string_view path;
  /// @brief Custom writer for arbitrary destinations. If set, path is ignored.
  std::optional<CustomWriter> custom_writer;
  /// @brief The profile to use for the MCAP file.
  std::string_view profile;
  /// @brief The size of each chunk in the MCAP file.
  uint64_t chunk_size = static_cast<uint64_t>(1024 * 768);
  /// @brief The compression algorithm to use for the MCAP file.
  McapCompression compression = McapCompression::Zstd;
  /// @brief Whether to use chunks in the MCAP file.
  bool use_chunks = true;
  /// @brief Whether to disable seeking in the MCAP file.
  bool disable_seeking = false;
  /// @brief Whether to emit statistics in the MCAP file.
  bool emit_statistics = true;
  /// @brief Whether to emit summary offsets in the MCAP file.
  bool emit_summary_offsets = true;
  /// @brief Whether to emit message indexes in the MCAP file.
  bool emit_message_indexes = true;
  /// @brief Whether to emit chunk indexes in the MCAP file.
  bool emit_chunk_indexes = true;
  /// @brief Whether to emit attachment indexes in the MCAP file.
  bool emit_attachment_indexes = true;
  /// @brief Whether to emit metadata indexes in the MCAP file.
  bool emit_metadata_indexes = true;
  /// @brief Whether to repeat channels in the MCAP file.
  bool repeat_channels = true;
  /// @brief Whether to repeat schemas in the MCAP file.
  bool repeat_schemas = true;
  /// @brief Whether to truncate the MCAP file.
  bool truncate = false;

  McapWriterOptions() = default;
};

/// @brief An MCAP writer, used to log messages to an MCAP file.
class McapWriter final {
public:
  /// @brief Create a new MCAP writer.
  ///
  /// @note Calls to create from multiple threads are safe,
  /// unless the same file path is given. Writing to an MCAP
  /// writer happens through channel logging, which is thread-safe.
  ///
  /// @param options The options for the MCAP writer.
  /// @return A new MCAP writer.
  static FoxgloveResult<McapWriter> create(const McapWriterOptions& options);

  /// @brief Stops logging events and flushes buffered data.
  FoxgloveError close();

  /// @brief Default move constructor.
  McapWriter(McapWriter&&) = default;
  /// @brief Default move assignment.
  McapWriter& operator=(McapWriter&&) = default;
  ~McapWriter() = default;

  McapWriter(const McapWriter&) = delete;
  McapWriter& operator=(const McapWriter&) = delete;

private:
  explicit McapWriter(foxglove_mcap_writer* writer);

  std::unique_ptr<foxglove_mcap_writer, foxglove_error (*)(foxglove_mcap_writer*)> impl_;
};

}  // namespace foxglove
