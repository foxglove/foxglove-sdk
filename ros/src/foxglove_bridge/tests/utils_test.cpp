#include <cstdint>
#include <limits>
#include <regex>
#include <vector>

#include <gtest/gtest.h>

#include <foxglove_bridge/param_utils.hpp>
#include <foxglove_bridge/utils.hpp>

using foxglove_bridge::compileTopicRegex;
using foxglove_bridge::DEFAULT_SUPPRESS_VIDEO_TRANSCODE_TOPIC_WHITELIST;
using foxglove_bridge::isWhitelisted;
using foxglove_bridge::saturatingToSizeT;

namespace {
// The shipped default for `suppress_video_transcode_topic_whitelist`, compiled exactly as the
// bridge compiles every topic pattern (compileTopicRegex → ECMAScript | icase). Building the tests
// off this — rather than a hand-copied literal — guards the actual default value AND the flags that
// ship, so a change to either is caught here.
std::vector<std::regex> defaultSuppressPatterns() {
  return {compileTopicRegex(DEFAULT_SUPPRESS_VIDEO_TRANSCODE_TOPIC_WHITELIST)};
}
}  // namespace

TEST(saturatingToSizeTTest, InRangeValuesPassThrough) {
  EXPECT_EQ(saturatingToSizeT(0), size_t{0});
  EXPECT_EQ(saturatingToSizeT(1), size_t{1});
  EXPECT_EQ(saturatingToSizeT(25), size_t{25});
  EXPECT_EQ(saturatingToSizeT(1024), size_t{1024});
}

TEST(saturatingToSizeTTest, ValueBelowMinClampsUp) {
  EXPECT_EQ(saturatingToSizeT(0, 1), size_t{1});
  EXPECT_EQ(saturatingToSizeT(-1, 0), size_t{0});
  EXPECT_EQ(saturatingToSizeT(std::numeric_limits<int64_t>::min(), 0), size_t{0});
}

TEST(saturatingToSizeTTest, NegativeValueClampsToDefaultMin) {
  EXPECT_EQ(saturatingToSizeT(-1), size_t{0});
  EXPECT_EQ(saturatingToSizeT(-1024), size_t{0});
}

TEST(saturatingToSizeTTest, LargeValueDoesNotWrapToSizeMax) {
  // Regression test: a naive std::clamp(value, 0, int64_t(size_t::max))
  // wraps the upper bound to -1 on 64-bit platforms, causing every
  // non-negative input to be returned as SIZE_MAX.
  EXPECT_EQ(saturatingToSizeT(1), size_t{1});
  EXPECT_EQ(saturatingToSizeT(25), size_t{25});
  EXPECT_EQ(saturatingToSizeT(1024), size_t{1024});
  EXPECT_NE(saturatingToSizeT(1), std::numeric_limits<size_t>::max());
  EXPECT_NE(saturatingToSizeT(1024), std::numeric_limits<size_t>::max());
}

TEST(saturatingToSizeTTest, ValueAtSizeTMaxIsPreserved) {
  // On 64-bit, size_t::max fits in uint64_t but not int64_t, so we can't
  // pass it directly; the largest representable int64_t input still must
  // round-trip cleanly.
  constexpr int64_t kLargeInput = std::numeric_limits<int64_t>::max();
  if constexpr (sizeof(size_t) >= sizeof(int64_t)) {
    EXPECT_EQ(saturatingToSizeT(kLargeInput), static_cast<size_t>(kLargeInput));
  } else {
    EXPECT_EQ(saturatingToSizeT(kLargeInput), std::numeric_limits<size_t>::max());
  }
}

// The default `suppress_video_transcode_topic_whitelist` pattern opts topics ending in
// `/compressedDepth` (the `compressed_depth_image_transport` suffix) out of video transcoding.
// `isWhitelisted` matches the whole topic (`std::regex_match`), so the suffix pattern needs the
// leading `.*`.
TEST(SuppressVideoTranscodeDefaultPatternTest, MatchesRosCompressedDepthTransport) {
  EXPECT_TRUE(isWhitelisted("/camera/depth/image_raw/compressedDepth", defaultSuppressPatterns()));
}

TEST(SuppressVideoTranscodeDefaultPatternTest, RejectsRegularCompressedImage) {
  EXPECT_FALSE(isWhitelisted("/camera/image_raw/compressed", defaultSuppressPatterns()));
}

TEST(SuppressVideoTranscodeDefaultPatternTest, RequiresSuffixNotSubstring) {
  EXPECT_FALSE(isWhitelisted("/compressedDepth/extra", defaultSuppressPatterns()));
}

// The bridge compiles every topic pattern case-insensitively (compileTopicRegex), so a
// `/CompressedDepth` suffix still matches. Guards that the default is exercised with the shipped
// flags, not a case-sensitive stand-in.
TEST(SuppressVideoTranscodeDefaultPatternTest, MatchesCaseInsensitively) {
  EXPECT_TRUE(isWhitelisted("/camera/depth/image_raw/CompressedDepth", defaultSuppressPatterns()));
}

TEST(SplitDefinitionsTest, EmptyMessageDefinition) {
  const std::string messageDef = "";
  std::istringstream stream(messageDef);
  const auto definitions = foxglove_bridge::splitMessageDefinitions(stream);
  ASSERT_EQ(definitions.size(), 1u);
  EXPECT_EQ(definitions[0], "");
}

TEST(SplitDefinitionsTest, ServiceDefinition) {
  const std::string messageDef =
    "bool data\n"
    "---\n"
    "bool success ";
  std::istringstream stream(messageDef);
  const auto definitions = foxglove_bridge::splitMessageDefinitions(stream);
  ASSERT_EQ(definitions.size(), 2u);
  EXPECT_EQ(definitions[0], "bool data");
  EXPECT_EQ(definitions[1], "bool success");
}

TEST(SplitDefinitionsTest, ServiceDefinitionEmptyRequest) {
  const std::string messageDef =
    "---\n"
    "bool success ";
  std::istringstream stream(messageDef);
  const auto definitions = foxglove_bridge::splitMessageDefinitions(stream);
  ASSERT_EQ(definitions.size(), 2u);
  EXPECT_EQ(definitions[0], "");
  EXPECT_EQ(definitions[1], "bool success");
}

TEST(SplitDefinitionsTest, ServiceDefinitionEmptyResponse) {
  const std::string messageDef =
    "bool data\n"
    "---";
  std::istringstream stream(messageDef);
  const auto definitions = foxglove_bridge::splitMessageDefinitions(stream);
  ASSERT_EQ(definitions.size(), 2u);
  EXPECT_EQ(definitions[0], "bool data");
  EXPECT_EQ(definitions[1], "");
}

TEST(SplitDefinitionsTest, ActionDefinition) {
  const std::string messageDef =
    "bool data\n"
    "---\n"
    "bool success\n"
    "---\n"
    "bool feedback";
  std::istringstream stream(messageDef);
  const auto definitions = foxglove_bridge::splitMessageDefinitions(stream);
  ASSERT_EQ(definitions.size(), 3u);
  EXPECT_EQ(definitions[0], "bool data");
  EXPECT_EQ(definitions[1], "bool success");
  EXPECT_EQ(definitions[2], "bool feedback");
}

TEST(SplitDefinitionsTest, ActionDefinitionNoGoal) {
  const std::string messageDef =
    "bool data\n"
    "---\n"
    "---\n"
    "bool feedback";
  std::istringstream stream(messageDef);
  const auto definitions = foxglove_bridge::splitMessageDefinitions(stream);
  ASSERT_EQ(definitions.size(), 3u);
  EXPECT_EQ(definitions[0], "bool data");
  EXPECT_EQ(definitions[1], "");
  EXPECT_EQ(definitions[2], "bool feedback");
}

TEST(SplitDefinitionsTest, HandleCarriageReturn) {
  const std::string messageDef =
    "---\r\n"
    "string device_name\n";
  std::istringstream stream(messageDef);
  const auto definitions = foxglove_bridge::splitMessageDefinitions(stream);
  ASSERT_EQ(definitions.size(), 2u);
  EXPECT_EQ(definitions[0], "");
  EXPECT_EQ(definitions[1], "string device_name");
}

int main(int argc, char** argv) {
  testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
