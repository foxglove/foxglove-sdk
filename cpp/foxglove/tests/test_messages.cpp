/**
 * Tests for the foxglove::messages namespace.
 *
 * This file tests that:
 * 1. The new foxglove::messages namespace works correctly
 * 2. The foxglove::schemas namespace alias provides backward compatibility
 * 3. Types from both namespaces are interchangeable
 */

#include <foxglove/messages.hpp>
#include <foxglove/schemas.hpp>

#include <catch2/catch_test_macros.hpp>

TEST_CASE("messages namespace types work correctly") {
  // Create types using the new messages namespace
  foxglove::messages::Vector3 vec{1.0, 2.0, 3.0};
  REQUIRE(vec.x == 1.0);
  REQUIRE(vec.y == 2.0);
  REQUIRE(vec.z == 3.0);

  foxglove::messages::Color color{1.0, 0.5, 0.0, 1.0};
  REQUIRE(color.r == 1.0);
  REQUIRE(color.g == 0.5);
  REQUIRE(color.b == 0.0);
  REQUIRE(color.a == 1.0);
}

TEST_CASE("schemas namespace alias works for backward compatibility") {
  // Create types using the deprecated schemas namespace alias
  foxglove::schemas::Vector3 vec{1.0, 2.0, 3.0};
  REQUIRE(vec.x == 1.0);
  REQUIRE(vec.y == 2.0);
  REQUIRE(vec.z == 3.0);

  foxglove::schemas::Color color{1.0, 0.5, 0.0, 1.0};
  REQUIRE(color.r == 1.0);
  REQUIRE(color.g == 0.5);
  REQUIRE(color.b == 0.0);
  REQUIRE(color.a == 1.0);
}

TEST_CASE("types from both namespaces are interchangeable") {
  // Create a type using messages namespace
  foxglove::messages::Vector3 messages_vec{1.0, 2.0, 3.0};

  // Assign to a reference using schemas namespace (should work due to alias)
  const foxglove::schemas::Vector3& schemas_vec = messages_vec;
  REQUIRE(schemas_vec.x == 1.0);
  REQUIRE(schemas_vec.y == 2.0);
  REQUIRE(schemas_vec.z == 3.0);

  // Create using schemas, use with messages
  foxglove::schemas::Color schemas_color{0.5, 0.5, 0.5, 1.0};
  const foxglove::messages::Color& messages_color = schemas_color;
  REQUIRE(messages_color.r == 0.5);
}

TEST_CASE("messages namespace schema() method works") {
  foxglove::Schema schema = foxglove::messages::Log::schema();
  REQUIRE(schema.name == "foxglove.Log");
  REQUIRE(schema.encoding == "protobuf");
  REQUIRE(schema.data != nullptr);
  REQUIRE(schema.data_len > 0);
}

TEST_CASE("messages namespace encode() method works") {
  foxglove::messages::Point2 point{10.0, 20.0};

  size_t capacity = 0;
  std::vector<uint8_t> buf(10);
  REQUIRE(point.encode(buf.data(), buf.size(), &capacity) == foxglove::FoxgloveError::BufferTooShort);
  buf.resize(capacity);
  REQUIRE(point.encode(buf.data(), buf.size(), &capacity) == foxglove::FoxgloveError::Ok);
  REQUIRE(capacity > 0);
}
