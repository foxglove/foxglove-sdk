#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

#include "channel.hpp"

/// @cond foxglove_internal
enum foxglove_error : uint8_t;
struct foxglove_mcap_writer;
struct FoxgloveCustomWriter;

foxglove_error foxglove_mcap_write_metadata(
  foxglove_mcap_writer* writer, const foxglove_string* name, const foxglove_key_value* metadata,
  size_t metadata_len
);
/// @endcond

/// The foxglove namespace.
namespace foxglove {

class Context;

/// @brief Custom writer for writing MCAP data to arbitrary destinations.
///
/// This provides a simple function pointer interface that matches the C API.
/// Users are responsible for managing the lifetime of user_data and ensuring
/// thread safety if needed.
struct CustomWriter {
  /// @brief Write function: write data to the custom destination
  /// @param data Pointer to data to write
  /// @param len Number of bytes to write
  /// @param error Pointer to error code (set to an error number defined in errno.h if write fails)
  /// @return Number of bytes actually written
  std::function<size_t(const uint8_t* data, size_t len, int* error)> write;

  /// @brief Flush function: ensure all buffered data is written
  /// @return 0 on success, an error number defined in errno.h if flush fails
  std::function<int()> flush;

  /// @brief Seek function: change the current position in the stream
  /// @param pos Position offset
  /// @param whence Seek origin (0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END)
  /// @param new_pos Pointer to store the new absolute position
  /// @return 0 on success, an error number defined in errno.h if seek fails
  std::function<int(int64_t pos, int whence, uint64_t* new_pos)> seek;
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
  /// @brief Optional channel filter to use for the MCAP file.
  SinkChannelFilterFn sink_channel_filter = {};

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

  /// @brief Write metadata to the MCAP file.
  ///
  /// Metadata consists of key-value string pairs associated with a name.
  /// If the range is empty, this method does nothing.
  ///
  /// @tparam Iterator An iterator type that dereferences to std::pair<std::string, std::string>
  /// @param name Name identifier for this metadata record
  /// @param begin Iterator to the beginning of the key-value pairs
  /// @param end Iterator to the end of the key-value pairs
  /// @return FoxgloveError::Ok on success, or an error code on failure
  template<typename Iterator>
  FoxgloveError writeMetadata(std::string_view name, Iterator begin, Iterator end);

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
  explicit McapWriter(
    foxglove_mcap_writer* writer,
    std::unique_ptr<SinkChannelFilterFn> sink_channel_filter = nullptr,
    std::unique_ptr<CustomWriter> custom_writer = nullptr
  );

  std::unique_ptr<SinkChannelFilterFn> sink_channel_filter_;
  std::unique_ptr<CustomWriter> custom_writer_;
  std::unique_ptr<foxglove_mcap_writer, foxglove_error (*)(foxglove_mcap_writer*)> impl_;
};

/// @copydoc McapWriter::writeMetadata
template<typename Iter>
FoxgloveError McapWriter::writeMetadata(std::string_view name, Iter begin, Iter end) {
  // Convert iterator range to C array of key-value pairs
  std::vector<foxglove_key_value> c_metadata;

  for (auto it = begin; it != end; ++it) {
    const auto& [key, value] = *it;
    foxglove_key_value kv;
    // data and length for both are necessary because foxglove_string is a C struct
    kv.key = {key.data(), key.length()};
    kv.value = {value.data(), value.length()};
    c_metadata.push_back(kv);
  }

  foxglove_string c_name = {name.data(), name.length()};

  foxglove_error error =
    foxglove_mcap_write_metadata(impl_.get(), &c_name, c_metadata.data(), c_metadata.size());

  return FoxgloveError(error);
}

}  // namespace foxglove
