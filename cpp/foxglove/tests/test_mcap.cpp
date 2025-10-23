#include <foxglove-c/foxglove-c.h>
#include <foxglove/arena.hpp>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <array>
#include <filesystem>
#include <fstream>
#include <optional>

#include "common/file_cleanup.hpp"

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;
using foxglove_tests::FileCleanup;

TEST_CASE("Open new file and close mcap writer") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());
  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("Open and truncate existing file") {
  FileCleanup cleanup("test.mcap");

  std::ofstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  // Write some dummy content
  const char* data = "MCAP0000";
  file.write(data, 8);
  file.close();

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());
  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("fail to open existing file if truncate=false") {
  FileCleanup cleanup("test.mcap");

  std::ofstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  // Write some dummy content
  const char* data = "MCAP0000";
  file.write(data, 8);
  file.close();

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::IoError);

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("fail to open existing file if create=true and truncate=false") {
  FileCleanup cleanup("test.mcap");

  std::ofstream file("test.mcap", std::ios::binary);
  REQUIRE(file.is_open());
  // Write some dummy content
  const char* data = "MCAP0000";
  file.write(data, 8);
  file.close();

  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::IoError);

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));
}

TEST_CASE("fail if file path is not valid utf-8") {
  FileCleanup cleanup("test.mcap");

  foxglove::McapWriterOptions options = {};
  options.path = "\x80\x80\x80\x80";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::Utf8Error);

  // Check test.mcap file does not exist
  REQUIRE(!std::filesystem::exists("test.mcap"));
}

std::string readFile(const std::string& path) {
  std::ifstream file(path, std::ios::binary);
  REQUIRE(file.is_open());
  return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

TEST_CASE("different contexts") {
  FileCleanup cleanup("test.mcap");
  auto context1 = foxglove::Context::create();
  auto context2 = foxglove::Context::create();

  // Create writer on context1
  foxglove::McapWriterOptions options;
  options.context = context1;
  options.path = "test.mcap";

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Log on context2 (should not be output to the file)
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example1", "json", schema, context2);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it does not contain the message
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, !ContainsSubstring("Hello, world!"));
}

TEST_CASE("specify profile") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = "test.mcap";
  options.profile = "test_profile";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example1", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto& channel = channel_result.value();
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the profile and library
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("test_profile"));
}

TEST_CASE("zstd compression") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = "test.mcap";
  options.compression = foxglove::McapCompression::Zstd;
  options.chunk_size = 10000;
  options.use_chunks = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example2", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the word "zstd"
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("zstd"));
}

TEST_CASE("lz4 compression") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = "test.mcap";
  options.compression = foxglove::McapCompression::Lz4;
  options.chunk_size = 10000;
  options.use_chunks = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example3", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto& channel = channel_result.value();
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  auto error = writer->close();
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Check if test.mcap file exists
  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that it contains the word "lz4"
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("lz4"));
}

TEST_CASE("Channel can outlive Schema") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  std::optional<foxglove::RawChannel> channel;
  {
    foxglove::Schema schema;
    schema.name = "ExampleSchema";
    schema.encoding = "unknown";
    std::string data = "FAKESCHEMA";
    schema.data = reinterpret_cast<const std::byte*>(data.data());
    schema.data_len = data.size();
    auto result = foxglove::RawChannel::create("example", "json", schema, context);
    REQUIRE(result.has_value());
    // Channel should copy the schema, so this modification has no effect on the output
    data[2] = 'I';
    data[3] = 'L';
    // Use emplace to construct the optional directly
    channel.emplace(std::move(result.value()));
  }

  const std::array<uint8_t, 3> data = {4, 5, 6};
  channel->log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  REQUIRE(std::filesystem::exists("test.mcap"));

  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, !ContainsSubstring("FAILSCHEMA"));
  REQUIRE_THAT(content, ContainsSubstring("FAKESCHEMA"));
}

namespace foxglove::schemas {
void imageAnnotationsToC(
  foxglove_image_annotations& dest, const ImageAnnotations& src, Arena& arena
);
}  // namespace foxglove::schemas

void convertToCAndCheck(const foxglove::schemas::ImageAnnotations& msg) {
  // Convert to C struct and then compare them
  foxglove::Arena arena;
  foxglove_image_annotations c_msg;
  imageAnnotationsToC(c_msg, msg, arena);

  // Compare the C struct with the original message
  REQUIRE(c_msg.circles_count == msg.circles.size());
  REQUIRE(c_msg.points_count == msg.points.size());
  REQUIRE(c_msg.texts_count == msg.texts.size());

  // Comapre circle annotation
  REQUIRE(c_msg.circles[0].timestamp->sec == msg.circles[0].timestamp->sec);
  REQUIRE(c_msg.circles[0].timestamp->nsec == msg.circles[0].timestamp->nsec);
  REQUIRE(c_msg.circles[0].position->x == msg.circles[0].position->x);
  REQUIRE(c_msg.circles[0].position->y == msg.circles[0].position->y);
  REQUIRE(c_msg.circles[0].diameter == msg.circles[0].diameter);
  REQUIRE(c_msg.circles[0].thickness == msg.circles[0].thickness);
  REQUIRE(c_msg.circles[0].fill_color->r == msg.circles[0].fill_color->r);
  REQUIRE(c_msg.circles[0].fill_color->g == msg.circles[0].fill_color->g);
  REQUIRE(c_msg.circles[0].fill_color->b == msg.circles[0].fill_color->b);
  REQUIRE(c_msg.circles[0].fill_color->a == msg.circles[0].fill_color->a);
  REQUIRE(c_msg.circles[0].outline_color->r == msg.circles[0].outline_color->r);
  REQUIRE(c_msg.circles[0].outline_color->g == msg.circles[0].outline_color->g);
  REQUIRE(c_msg.circles[0].outline_color->b == msg.circles[0].outline_color->b);
  REQUIRE(c_msg.circles[0].outline_color->a == msg.circles[0].outline_color->a);

  // Compare points annotation
  REQUIRE(c_msg.points[0].timestamp->sec == msg.points[0].timestamp->sec);
  REQUIRE(c_msg.points[0].timestamp->nsec == msg.points[0].timestamp->nsec);
  REQUIRE(static_cast<uint8_t>(c_msg.points[0].type) == static_cast<uint8_t>(msg.points[0].type));
  REQUIRE(c_msg.points[0].points_count == msg.points[0].points.size());
  for (size_t i = 0; i < msg.points[0].points.size(); ++i) {
    REQUIRE(c_msg.points[0].points[i].x == msg.points[0].points[i].x);
    REQUIRE(c_msg.points[0].points[i].y == msg.points[0].points[i].y);
  }
  REQUIRE(c_msg.points[0].outline_color->r == msg.points[0].outline_color->r);
  REQUIRE(c_msg.points[0].outline_color->g == msg.points[0].outline_color->g);
  REQUIRE(c_msg.points[0].outline_color->b == msg.points[0].outline_color->b);
  REQUIRE(c_msg.points[0].outline_color->a == msg.points[0].outline_color->a);
  REQUIRE(c_msg.points[0].outline_colors_count == msg.points[0].outline_colors.size());
  for (size_t i = 0; i < msg.points[0].outline_colors.size(); ++i) {
    REQUIRE(c_msg.points[0].outline_colors[i].r == msg.points[0].outline_colors[i].r);
    REQUIRE(c_msg.points[0].outline_colors[i].g == msg.points[0].outline_colors[i].g);
    REQUIRE(c_msg.points[0].outline_colors[i].b == msg.points[0].outline_colors[i].b);
    REQUIRE(c_msg.points[0].outline_colors[i].a == msg.points[0].outline_colors[i].a);
  }
  REQUIRE(c_msg.points[0].fill_color->r == msg.points[0].fill_color->r);
  REQUIRE(c_msg.points[0].fill_color->g == msg.points[0].fill_color->g);
  REQUIRE(c_msg.points[0].fill_color->b == msg.points[0].fill_color->b);
  REQUIRE(c_msg.points[0].fill_color->a == msg.points[0].fill_color->a);
  REQUIRE(c_msg.points[0].thickness == msg.points[0].thickness);

  // Compare text annotation
  REQUIRE(c_msg.texts[0].timestamp->sec == msg.texts[0].timestamp->sec);
  REQUIRE(c_msg.texts[0].timestamp->nsec == msg.texts[0].timestamp->nsec);
  REQUIRE(c_msg.texts[0].position->x == msg.texts[0].position->x);
  REQUIRE(c_msg.texts[0].position->y == msg.texts[0].position->y);
  REQUIRE(c_msg.texts[0].text.data == msg.texts[0].text.data());
  REQUIRE(c_msg.texts[0].text.len == msg.texts[0].text.size());
  REQUIRE(c_msg.texts[0].font_size == msg.texts[0].font_size);
  REQUIRE(c_msg.texts[0].text_color->r == msg.texts[0].text_color->r);
  REQUIRE(c_msg.texts[0].text_color->g == msg.texts[0].text_color->g);
  REQUIRE(c_msg.texts[0].text_color->b == msg.texts[0].text_color->b);
  REQUIRE(c_msg.texts[0].text_color->a == msg.texts[0].text_color->a);
  REQUIRE(c_msg.texts[0].background_color->r == msg.texts[0].background_color->r);
  REQUIRE(c_msg.texts[0].background_color->g == msg.texts[0].background_color->g);
  REQUIRE(c_msg.texts[0].background_color->b == msg.texts[0].background_color->b);
  REQUIRE(c_msg.texts[0].background_color->a == msg.texts[0].background_color->a);
}

TEST_CASE("ImageAnnotations channel") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = "test.mcap";
  options.compression = foxglove::McapCompression::None;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  auto channel_result = foxglove::schemas::ImageAnnotationsChannel::create("example", context);
  REQUIRE(channel_result.has_value());
  auto channel = std::move(channel_result.value());

  // Prepare ImageAnnotations message
  foxglove::schemas::ImageAnnotations msg;

  // Add a circle annotation
  foxglove::schemas::CircleAnnotation circle;
  circle.timestamp = foxglove::schemas::Timestamp{1000000000, 500000000};
  circle.position = foxglove::schemas::Point2{10.0, 20.0};
  circle.diameter = 15.0;
  circle.thickness = 2.0;
  circle.fill_color = foxglove::schemas::Color{1.0, 0.5, 0.3, 0.8};
  circle.outline_color = foxglove::schemas::Color{0.1, 0.2, 0.9, 1.0};
  msg.circles.push_back(circle);

  // Add a points annotation
  foxglove::schemas::PointsAnnotation points;
  points.timestamp = foxglove::schemas::Timestamp{1000000000, 500000000};
  points.type = foxglove::schemas::PointsAnnotation::PointsAnnotationType::LINE_STRIP;
  points.points.push_back(foxglove::schemas::Point2{5.0, 10.0});
  points.points.push_back(foxglove::schemas::Point2{15.0, 25.0});
  points.points.push_back(foxglove::schemas::Point2{30.0, 15.0});
  points.outline_color = foxglove::schemas::Color{0.8, 0.2, 0.3, 1.0};
  points.outline_colors.push_back(foxglove::schemas::Color{0.9, 0.1, 0.2, 1.0});
  points.fill_color = foxglove::schemas::Color{0.2, 0.8, 0.3, 0.5};
  points.thickness = 3.0;
  msg.points.push_back(points);

  // Add a text annotation
  foxglove::schemas::TextAnnotation text;
  text.timestamp = foxglove::schemas::Timestamp{1000000000, 500000000};
  text.position = foxglove::schemas::Point2{50.0, 60.0};
  text.text = "Sample text";
  text.font_size = 14.0;
  text.text_color = foxglove::schemas::Color{0.0, 0.0, 0.0, 1.0};
  text.background_color = foxglove::schemas::Color{1.0, 1.0, 1.0, 0.7};
  msg.texts.push_back(text);

  convertToCAndCheck(msg);

  channel.log(msg);

  writer->close();

  REQUIRE(std::filesystem::exists("test.mcap"));

  // Check that the file contains our annotations
  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, ContainsSubstring("Sample text"));
  REQUIRE_THAT(content, ContainsSubstring("ImageAnnotations"));
}

TEST_CASE("MCAP Channel filtering") {
  FileCleanup file_1("test-1.mcap");
  FileCleanup file_2("test-2.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions opts_1;
  opts_1.context = context;
  opts_1.compression = foxglove::McapCompression::None;
  opts_1.path = "test-1.mcap";
  opts_1.sink_channel_filter = [](foxglove::ChannelDescriptor&& channel) -> bool {
    return channel.topic() == "/1";
  };
  auto writer_res_1 = foxglove::McapWriter::create(opts_1);
  if (!writer_res_1.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_res_1.error()) << '\n';
  }
  REQUIRE(writer_res_1.has_value());
  auto writer_1 = std::move(writer_res_1.value());

  foxglove::McapWriterOptions opts_2;
  opts_2.context = context;
  opts_2.compression = foxglove::McapCompression::None;
  opts_2.path = "test-2.mcap";
  opts_2.sink_channel_filter = [](foxglove::ChannelDescriptor&& channel) -> bool {
    // Only log to topic /2, and validate the schema while we're at it
    if (channel.topic() == "/2") {
      REQUIRE(channel.schema().has_value());
      REQUIRE(channel.schema().value().name == "Topic2Schema");
      REQUIRE(channel.schema().value().encoding == "fake-encoding");
      REQUIRE(channel.metadata().has_value());
      REQUIRE(channel.metadata().value().size() == 2);
      REQUIRE(channel.metadata().value().at("key1") == "value1");
      REQUIRE(channel.metadata().value().at("key2") == "value2");
      return true;
    }
    return false;
  };
  auto writer_res_2 = foxglove::McapWriter::create(opts_2);
  REQUIRE(writer_res_2.has_value());
  auto writer_2 = std::move(writer_res_2.value());

  {
    auto result = foxglove::RawChannel::create("/1", "json", std::nullopt, context);
    REQUIRE(result.has_value());
    auto channel = std::move(result.value());
    std::string data = "Topic 1 msg";
    channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());
  }
  {
    foxglove::Schema topic2Schema;
    topic2Schema.name = "Topic2Schema";
    topic2Schema.encoding = "fake-encoding";
    std::string schemaData = "FAKESCHEMA";
    topic2Schema.data = reinterpret_cast<const std::byte*>(schemaData.data());
    topic2Schema.data_len = schemaData.size();

    std::map<std::string, std::string> metadata = {{"key1", "value1"}, {"key2", "value2"}};

    auto result =
      foxglove::RawChannel::create("/2", "json", std::move(topic2Schema), context, metadata);
    REQUIRE(result.has_value());
    auto channel = std::move(result.value());
    std::string data = "Topic 2 msg";
    channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());
  }

  writer_1.close();
  writer_2.close();

  // Check that the file contains the correct filtered messages
  std::string content = readFile("test-1.mcap");
  std::cerr << "test-1 content.length: " << content.length() << "\n";
  REQUIRE_THAT(content, ContainsSubstring("Topic 1 msg"));
  REQUIRE_THAT(content, !ContainsSubstring("Topic 2 msg"));

  content = readFile("test-2.mcap");
  REQUIRE_THAT(content, !ContainsSubstring("Topic 1 msg"));
  REQUIRE_THAT(content, ContainsSubstring("Topic 2 msg"));
}
