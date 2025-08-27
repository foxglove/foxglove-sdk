#include <foxglove-c/foxglove-c.h>
#include <foxglove/arena.hpp>
#include <foxglove/schemas.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <string>

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;
using namespace foxglove;
using namespace foxglove::schemas;

namespace foxglove::schemas {
void triangleListPrimitiveToC(
  foxglove_triangle_list_primitive& dest, const TriangleListPrimitive& src, Arena& arena
);
}  // namespace foxglove::schemas

TEST_CASE("triangle list primitive to c") {
  Arena arena;
  foxglove_triangle_list_primitive dest;
  TriangleListPrimitive src;

  // Populate the pose
  src.pose = Pose{};
  src.pose->position = Vector3{1.0, 2.0, 3.0};
  src.pose->orientation = Quaternion{0.1, 0.2, 0.3, 0.4};

  // Add at least one triangle (3 points)
  src.points.push_back(Point3{0.0, 0.0, 0.0});
  src.points.push_back(Point3{1.0, 0.0, 0.0});
  src.points.push_back(Point3{0.5, 1.0, 0.0});

  // Set a solid color for the whole shape
  src.color = Color{1.0, 0.0, 0.0, 1.0};

  // Add per-vertex colors (same length as points)
  src.colors.push_back(Color{1.0, 0.0, 0.0, 1.0});
  src.colors.push_back(Color{0.0, 1.0, 0.0, 1.0});
  src.colors.push_back(Color{0.0, 0.0, 1.0, 1.0});

  // Add some indices
  src.indices.push_back(0);
  src.indices.push_back(1);
  src.indices.push_back(2);

  foxglove::schemas::triangleListPrimitiveToC(dest, src, arena);

  // Verify the conversion worked
  REQUIRE(dest.pose != nullptr);
  REQUIRE(dest.pose->position != nullptr);
  REQUIRE(dest.pose->position->x == 1.0);
  REQUIRE(dest.pose->position->y == 2.0);
  REQUIRE(dest.pose->position->z == 3.0);
  REQUIRE(dest.pose->orientation != nullptr);
  REQUIRE(dest.pose->orientation->x == 0.1);
  REQUIRE(dest.pose->orientation->y == 0.2);
  REQUIRE(dest.pose->orientation->z == 0.3);
  REQUIRE(dest.pose->orientation->w == 0.4);

  REQUIRE(dest.points_count == 3);
  REQUIRE(dest.points[0].x == 0.0);
  REQUIRE(dest.points[1].x == 1.0);
  REQUIRE(dest.points[2].y == 1.0);

  REQUIRE(dest.color != nullptr);
  REQUIRE(dest.color->r == 1.0);
  REQUIRE(dest.color->g == 0.0);

  REQUIRE(dest.colors_count == 3);
  REQUIRE(dest.colors[0].r == 1.0);
  REQUIRE(dest.colors[1].g == 1.0);
  REQUIRE(dest.colors[2].b == 1.0);

  REQUIRE(dest.indices_count == 3);
  REQUIRE(dest.indices[0] == 0);
  REQUIRE(dest.indices[1] == 1);
  REQUIRE(dest.indices[2] == 2);
}

TEST_CASE("triangle list primitive to protobuf") {
  TriangleListPrimitive msg;

  // Populate the pose
  msg.pose = Pose{};
  msg.pose->position = Vector3{1.0, 2.0, 3.0};
  msg.pose->orientation = Quaternion{0.1, 0.2, 0.3, 0.4};

  // Add at least one triangle (3 points)
  msg.points.push_back(Point3{0.0, 0.0, 0.0});
  msg.points.push_back(Point3{1.0, 0.0, 0.0});
  msg.points.push_back(Point3{0.5, 1.0, 0.0});

  // Set a solid color for the whole shape
  msg.color = Color{1.0, 0.0, 0.0, 1.0};

  // Add per-vertex colors (same length as points)
  msg.colors.push_back(Color{1.0, 0.0, 0.0, 1.0});
  msg.colors.push_back(Color{0.0, 1.0, 0.0, 1.0});
  msg.colors.push_back(Color{0.0, 0.0, 1.0, 1.0});

  // Add some indices
  msg.indices.push_back(0);
  msg.indices.push_back(1);
  msg.indices.push_back(2);

  size_t capacity = 0;
  std::vector<uint8_t> buf(10);
  REQUIRE(msg.encode(buf.data(), buf.size(), &capacity) == FoxgloveError::BufferTooShort);
  buf.resize(capacity);
  REQUIRE(msg.encode(buf.data(), buf.size(), &capacity) == FoxgloveError::Ok);
  REQUIRE(capacity == buf.size());
  REQUIRE(capacity > 0);
}

TEST_CASE("triangle list primitive returns a schema") {
  Schema schema = TriangleListPrimitive::schema();
  REQUIRE(schema.name == "foxglove.TriangleListPrimitive");
  REQUIRE(schema.encoding == "protobuf");
  REQUIRE(schema.data != NULL);
  REQUIRE(schema.data_len > 0);
}
