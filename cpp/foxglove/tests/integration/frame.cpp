#include "frame.hpp"

#include <cstring>

namespace foxglove_integration {

std::vector<uint8_t> frame_text_message(const uint8_t* data, size_t len) {
  auto frame_len = static_cast<uint32_t>(len);
  std::vector<uint8_t> buf;
  buf.reserve(HEADER_SIZE + len);
  buf.push_back(static_cast<uint8_t>(OpCode::Text));
  buf.push_back(static_cast<uint8_t>(frame_len & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 8) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 16) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 24) & 0xFF));
  buf.insert(buf.end(), data, data + len);
  return buf;
}

std::vector<uint8_t> frame_text_message(const std::string& json) {
  return frame_text_message(reinterpret_cast<const uint8_t*>(json.data()), json.size());
}

std::vector<uint8_t> frame_binary_message(const uint8_t* data, size_t len) {
  auto frame_len = static_cast<uint32_t>(len);
  std::vector<uint8_t> buf;
  buf.reserve(HEADER_SIZE + len);
  buf.push_back(static_cast<uint8_t>(OpCode::Binary));
  buf.push_back(static_cast<uint8_t>(frame_len & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 8) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 16) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 24) & 0xFF));
  buf.insert(buf.end(), data, data + len);
  return buf;
}

std::optional<ParseResult> try_parse_frame(const uint8_t* data, size_t len) {
  if (len < HEADER_SIZE) {
    return std::nullopt;
  }
  uint8_t op = data[0];
  if (op != static_cast<uint8_t>(OpCode::Text) && op != static_cast<uint8_t>(OpCode::Binary)) {
    throw std::runtime_error("unknown opcode: " + std::to_string(op));
  }
  uint32_t payload_len = 0;
  std::memcpy(&payload_len, data + 1, 4);
  size_t total = HEADER_SIZE + payload_len;
  if (len < total) {
    return std::nullopt;
  }
  if (payload_len == 0) {
    throw std::runtime_error("empty frame payload");
  }
  Frame frame;
  frame.op_code = static_cast<OpCode>(op);
  frame.payload.assign(data + HEADER_SIZE, data + total);
  return ParseResult{std::move(frame), total};
}

}  // namespace foxglove_integration
