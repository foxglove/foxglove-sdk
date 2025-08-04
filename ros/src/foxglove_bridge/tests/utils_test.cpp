#include <gtest/gtest.h>

#include <foxglove_bridge/utils.hpp>

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
