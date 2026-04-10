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

struct Frame {
  OpCode op_code;
  std::vector<uint8_t> payload;
};

constexpr size_t HEADER_SIZE = 5;

std::vector<uint8_t> frame_text_message(const uint8_t* data, size_t len);
std::vector<uint8_t> frame_text_message(const std::string& json);
std::vector<uint8_t> frame_binary_message(const uint8_t* data, size_t len);

struct ParseResult {
  Frame frame;
  size_t bytes_consumed;
};

/// Attempts to parse a single frame from the accumulated buffer.
/// Returns std::nullopt if more data is needed.
/// Throws on invalid data.
std::optional<ParseResult> try_parse_frame(const uint8_t* data, size_t len);

}  // namespace foxglove_integration
