#include <gtest/gtest.h>

#include <foxglove_bridge/base64.hpp>

TEST(Base64Test, EncodingTest) {
  constexpr char arr[] = {'A', 'B', 'C', 'D'};
  const std::string_view sv(arr, sizeof(arr));
  const std::string b64encoded = foxglove_ws::base64Encode(sv);
  EXPECT_EQ(b64encoded, "QUJDRA==");
}

TEST(Base64Test, DecodeTest) {
  const std::vector<unsigned char> expectedVal = {0x00, 0xFF, 0x01, 0xFE};
  EXPECT_EQ(foxglove_ws::base64Decode("AP8B/g=="), expectedVal);
}

TEST(Base64Test, DecodeInvalidStringTest) {
  // String length not multiple of 4
  EXPECT_THROW(foxglove_ws::base64Decode("faefa"), std::runtime_error);
  // Invalid characters
  EXPECT_THROW(foxglove_ws::base64Decode("fa^ef a"), std::runtime_error);
}

int main(int argc, char** argv) {
  testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
