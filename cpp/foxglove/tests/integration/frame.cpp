#include "frame.hpp"

namespace foxglove_integration {

std::vector<uint8_t> bytestream_frame_text_message(const uint8_t* data, size_t len) {
  auto frame_len = static_cast<uint32_t>(len);
  std::vector<uint8_t> buf;
  buf.reserve(BYTESTREAM_HEADER_SIZE + len);
  buf.push_back(static_cast<uint8_t>(OpCode::Text));
  buf.push_back(static_cast<uint8_t>(frame_len & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 8) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 16) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 24) & 0xFF));
  buf.insert(buf.end(), data, data + len);
  return buf;
}

std::vector<uint8_t> bytestream_frame_text_message(const std::string& json) {
  return bytestream_frame_text_message(reinterpret_cast<const uint8_t*>(json.data()), json.size());
}

std::vector<uint8_t> bytestream_frame_binary_message(const uint8_t* data, size_t len) {
  auto frame_len = static_cast<uint32_t>(len);
  std::vector<uint8_t> buf;
  buf.reserve(BYTESTREAM_HEADER_SIZE + len);
  buf.push_back(static_cast<uint8_t>(OpCode::Binary));
  buf.push_back(static_cast<uint8_t>(frame_len & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 8) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 16) & 0xFF));
  buf.push_back(static_cast<uint8_t>((frame_len >> 24) & 0xFF));
  buf.insert(buf.end(), data, data + len);
  return buf;
}

std::optional<ByteStreamParseResult> try_parse_bytestream_frame(const uint8_t* data, size_t len) {
  if (len < BYTESTREAM_HEADER_SIZE) {
    return std::nullopt;
  }
  uint8_t op = data[0];
  if (op != static_cast<uint8_t>(OpCode::Text) && op != static_cast<uint8_t>(OpCode::Binary)) {
    throw std::runtime_error("unknown opcode: " + std::to_string(op));
  }
  uint32_t payload_len = read_u32_le(data + 1);
  size_t total = BYTESTREAM_HEADER_SIZE + payload_len;
  if (len < total) {
    return std::nullopt;
  }
  // We don't send messages with an empty payload
  if (payload_len == 0) {
    throw std::runtime_error("empty frame payload");
  }
  ByteStreamFrame frame;
  frame.op_code = static_cast<OpCode>(op);
  frame.payload.assign(data + BYTESTREAM_HEADER_SIZE, data + total);
  return ByteStreamParseResult{std::move(frame), total};
}

}  // namespace foxglove_integration
