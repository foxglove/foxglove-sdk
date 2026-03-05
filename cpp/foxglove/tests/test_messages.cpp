// Verify that foxglove::messages is a working alias for foxglove::schemas.

#include <foxglove/messages.hpp>
#include <foxglove/schemas.hpp>

#include <catch2/catch_test_macros.hpp>

#include <array>

using namespace foxglove;

// NOLINTBEGIN(cppcoreguidelines-avoid-do-while)

TEST_CASE("messages alias types are identical to schemas types") {
  messages::Vector3 v{1.0, 2.0, 3.0};
  schemas::Vector3& v_ref = v;
  REQUIRE(v_ref.x == 1.0);
  REQUIRE(v_ref.y == 2.0);
  REQUIRE(v_ref.z == 3.0);
}

TEST_CASE("messages alias supports construction and encoding") {
  messages::Log log;
  log.message = "test message";
  log.level = messages::Log::LogLevel::INFO;

  std::array<uint8_t, 256> buf{};
  size_t encoded_len = 0;
  auto err = log.encode(buf.data(), buf.size(), &encoded_len);
  REQUIRE(err == FoxgloveError::Ok);
  REQUIRE(encoded_len > 0);
}

TEST_CASE("messages alias provides schema access") {
  auto schema = messages::Log::schema();
  REQUIRE(schema.name == "foxglove.Log");
  REQUIRE(schema.encoding == "protobuf");
  REQUIRE(schema.data_len > 0);
}

// NOLINTEND(cppcoreguidelines-avoid-do-while)
