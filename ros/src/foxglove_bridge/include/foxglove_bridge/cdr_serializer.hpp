#pragma once

#include <string>

#include <rosx_introspection/serializer.hpp>

namespace foxglove_bridge {

/// Reports whether the rosx_introspection ROS 2 serializer omits the null terminator when writing
/// a string.
///
/// ROS 2 middlewares expect a string to be encoded as a length of `size() + 1`, followed by the
/// characters and a trailing null byte. rosx_introspection writes the characters alone, prefixed
/// by a length that excludes the terminator, which Cyclone DDS rejects outright
/// (https://github.com/facontidavide/rosx_introspection/issues/40).
///
/// This is probed at runtime rather than assumed, so that a fixed rosx_introspection doesn't leave
/// us writing two terminators. The encoded buffer holds the 4-byte encapsulation header and a
/// 4-byte length, followed by the characters, so a one-character string occupies 9 bytes without a
/// terminator and 10 bytes with one.
inline bool serializerOmitsNullTerminator() {
  RosMsgParser::ROS2_Serializer probe;
  probe.reset();
  probe.serializeString("x");
  return probe.getBufferSize() == 9;
}

/// CDR serializer used to convert client-published JSON messages to ROS 2 CDR.
class CdrSerializer : public RosMsgParser::ROS2_Serializer {
public:
  void serializeString(const std::string& str) override {
    static const bool appendTerminator = serializerOmitsNullTerminator();
    if (!appendTerminator) {
      RosMsgParser::ROS2_Serializer::serializeString(str);
      return;
    }
    std::string terminated = str;
    terminated.push_back('\0');
    RosMsgParser::ROS2_Serializer::serializeString(terminated);
  }
};

}  // namespace foxglove_bridge
