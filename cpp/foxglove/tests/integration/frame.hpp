#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <stdexcept>
#include <vector>

namespace foxglove_integration {

enum class OpCode : uint8_t {
  Text = 1,
  Binary = 2,
};

/// Reads a little-endian uint16_t from a byte buffer. Caller must ensure that
/// at least 2 bytes are readable from `data`.
inline uint16_t read_u16_le(const uint8_t* data) {
  return static_cast<uint16_t>(data[0]) | (static_cast<uint16_t>(data[1]) << 8);
}

/// Reads a little-endian uint32_t from a byte buffer. Caller must ensure that
/// at least 4 bytes are readable from `data`. Endian-safe on both LE and BE
/// hosts (do not use std::memcpy into a uint32_t for LE-on-the-wire data).
inline uint32_t read_u32_le(const uint8_t* data) {
  return static_cast<uint32_t>(data[0]) | (static_cast<uint32_t>(data[1]) << 8) |
         (static_cast<uint32_t>(data[2]) << 16) | (static_cast<uint32_t>(data[3]) << 24);
}

/// Reads a little-endian uint64_t from a byte buffer. Caller must ensure that
/// at least 8 bytes are readable from `data`. Endian-safe on both LE and BE
/// hosts (do not use std::memcpy into a uint64_t for LE-on-the-wire data).
inline uint64_t read_u64_le(const uint8_t* data) {
  return static_cast<uint64_t>(data[0]) | (static_cast<uint64_t>(data[1]) << 8) |
         (static_cast<uint64_t>(data[2]) << 16) | (static_cast<uint64_t>(data[3]) << 24) |
         (static_cast<uint64_t>(data[4]) << 32) | (static_cast<uint64_t>(data[5]) << 40) |
         (static_cast<uint64_t>(data[6]) << 48) | (static_cast<uint64_t>(data[7]) << 56);
}

struct ByteStreamFrame {
  OpCode op_code;
  std::vector<uint8_t> payload;
};

constexpr size_t BYTESTREAM_HEADER_SIZE = 5;

std::vector<uint8_t> bytestream_frame_text_message(const uint8_t* data, size_t len);
std::vector<uint8_t> bytestream_frame_text_message(const std::string& json);
std::vector<uint8_t> bytestream_frame_binary_message(const uint8_t* data, size_t len);

struct ByteStreamParseResult {
  ByteStreamFrame frame;
  size_t bytes_consumed;
};

/// Attempts to parse a single ByteStream frame from the accumulated buffer.
/// Returns std::nullopt if more data is needed.
/// Throws on invalid data.
std::optional<ByteStreamParseResult> try_parse_bytestream_frame(const uint8_t* data, size_t len);

}  // namespace foxglove_integration
