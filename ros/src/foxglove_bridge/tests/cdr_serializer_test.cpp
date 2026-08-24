#include <cstdint>
#include <string>
#include <vector>

#include <gtest/gtest.h>
#include <rosx_introspection/ros_parser.hpp>

#include <foxglove_bridge/cdr_serializer.hpp>

namespace {

std::vector<uint8_t> toBytes(const RosMsgParser::Serializer& serializer) {
  const auto* data = reinterpret_cast<const uint8_t*>(serializer.getBufferData());
  return std::vector<uint8_t>(data, data + serializer.getBufferSize());
}

// Converts a JSON message to CDR the same way the bridge does for messages published by a client
// on a channel advertised with the "json" encoding.
std::vector<uint8_t> jsonToCdr(const std::string& topicType, const std::string& schema,
                               const std::string& json) {
  const RosMsgParser::Parser parser("topic", RosMsgParser::ROSType(topicType), schema);
  foxglove_bridge::CdrSerializer serializer;
  serializer.reset();
  parser.serializeFromJson(json, &serializer);
  return toBytes(serializer);
}

// Little-endian, plain CDR encapsulation header.
constexpr uint8_t H[] = {0x00, 0x01, 0x00, 0x00};

}  // namespace

// CdrSerializer only appends a terminator if the underlying serializer omits it. The probe that
// decides this recognizes exactly two encodings; any other output means rosx_introspection changed
// in a way the probe can no longer reason about.
TEST(CdrSerializerTest, probeMatchesUnderlyingSerializer) {
  RosMsgParser::ROS2_Serializer serializer;
  serializer.reset();
  serializer.serializeString("x");

  const std::vector<uint8_t> unterminated = {H[0], H[1], H[2], H[3], 1, 0, 0, 0, 'x'};
  const std::vector<uint8_t> terminated = {H[0], H[1], H[2], H[3], 2, 0, 0, 0, 'x', '\0'};
  if (foxglove_bridge::serializerOmitsNullTerminator()) {
    EXPECT_EQ(unterminated, toBytes(serializer));
  } else {
    EXPECT_EQ(terminated, toBytes(serializer));
  }
}

TEST(CdrSerializerTest, serializesString) {
  const auto cdr = jsonToCdr("std_msgs/msg/String", "string data", R"({"data": "hello world"})");

  // The length prefix (12) includes the trailing null byte.
  const std::vector<uint8_t> expected = {
    H[0], H[1], H[2], H[3],                                           //
    12,   0,    0,    0,                                              //
    'h',  'e',  'l',  'l',  'o', ' ', 'w', 'o', 'r', 'l', 'd', '\0',  //
  };
  EXPECT_EQ(expected, cdr);
}

TEST(CdrSerializerTest, serializesEmptyString) {
  const auto cdr = jsonToCdr("std_msgs/msg/String", "string data", R"({"data": ""})");

  const std::vector<uint8_t> expected = {H[0], H[1], H[2], H[3], 1, 0, 0, 0, '\0'};
  EXPECT_EQ(expected, cdr);
}

TEST(CdrSerializerTest, serializesOmittedStringAsEmpty) {
  const auto cdr = jsonToCdr("std_msgs/msg/String", "string data", "{}");

  const std::vector<uint8_t> expected = {H[0], H[1], H[2], H[3], 1, 0, 0, 0, '\0'};
  EXPECT_EQ(expected, cdr);
}

TEST(CdrSerializerTest, alignsFieldFollowingString) {
  const auto cdr = jsonToCdr("test_msgs/msg/StringAndInt", "string data\nint32 value",
                             R"({"data": "abcd", "value": 42})");

  // "abcd\0" is 5 bytes, so the int32 that follows is preceded by 3 bytes of padding.
  const std::vector<uint8_t> expected = {
    H[0], H[1], H[2], H[3],        //
    5,    0,    0,    0,           //
    'a',  'b',  'c',  'd',  '\0',  //
    0,    0,    0,                 // padding
    42,   0,    0,    0,           //
  };
  EXPECT_EQ(expected, cdr);
}

TEST(CdrSerializerTest, serializesStringArray) {
  const auto cdr =
    jsonToCdr("test_msgs/msg/StringArray", "string[] data", R"({"data": ["ab", "cd"]})");

  const std::vector<uint8_t> expected = {
    H[0], H[1], H[2], H[3],  //
    2,    0,    0,    0,     // array length
    3,    0,    0,    0,     //
    'a',  'b',  '\0',        //
    0,                       // padding
    3,    0,    0,    0,     //
    'c',  'd',  '\0',        //
  };
  EXPECT_EQ(expected, cdr);
}
