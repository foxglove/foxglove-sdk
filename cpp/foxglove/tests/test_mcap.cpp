#include <foxglove/arena.hpp>
#include <foxglove/channel.hpp>
#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <array>
#include <filesystem>
#include <fstream>
#include <optional>
#include <random>

#include "../src/mcap_internal.hpp"
#include "common/file_cleanup.hpp"
#include "common/test_helpers.hpp"

using Catch::Matchers::ContainsSubstring;
using Catch::Matchers::Equals;
using foxglove_tests::FileCleanup;
using foxglove_tests::requireValue;

struct McapTestFile {
  McapTestFile()
      : cleanup_("test_mcap_" + std::to_string(std::random_device{}()) + ".mcap") {}
  [[nodiscard]] const std::string& path() const {
    return cleanup_.path();
  }

private:
  FileCleanup cleanup_;
};

TEST_CASE_METHOD(McapTestFile, "Open new file and close mcap writer") {
  foxglove::McapWriterOptions options = {};
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());
  writer->close();

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));
}

TEST_CASE_METHOD(McapTestFile, "Open and truncate existing file") {
  {
    std::ofstream ofs(path(), std::ios::binary);
    REQUIRE(ofs.is_open());
    // Write some dummy content
    const char* data = "MCAP0000";
    ofs.write(data, 8);
  }

  foxglove::McapWriterOptions options = {};
  options.path = path();
  options.truncate = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());
  writer->close();

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));
}

TEST_CASE_METHOD(McapTestFile, "fail to open existing file if truncate=false") {
  {
    std::ofstream ofs(path(), std::ios::binary);
    REQUIRE(ofs.is_open());
    // Write some dummy content
    const char* data = "MCAP0000";
    ofs.write(data, 8);
  }

  foxglove::McapWriterOptions options = {};
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::IoError);

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));
}

TEST_CASE_METHOD(McapTestFile, "fail to open existing file if create=true and truncate=false") {
  {
    std::ofstream ofs(path(), std::ios::binary);
    REQUIRE(ofs.is_open());
    // Write some dummy content
    const char* data = "MCAP0000";
    ofs.write(data, 8);
  }

  foxglove::McapWriterOptions options = {};
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::IoError);

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));
}

TEST_CASE("fail if file path is not valid utf-8") {
  foxglove::McapWriterOptions options = {};
  options.path = "\x80\x80\x80\x80";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(!writer.has_value());
  REQUIRE(writer.error() == foxglove::FoxgloveError::Utf8Error);
}

std::string readFile(const std::string& path) {
  std::ifstream file(path, std::ios::binary);
  REQUIRE(file.is_open());
  return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

TEST_CASE_METHOD(McapTestFile, "different contexts") {
  auto context1 = foxglove::Context::create();
  auto context2 = foxglove::Context::create();

  // Create writer on context1
  foxglove::McapWriterOptions options;
  options.context = context1;
  options.path = path();

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Log on context2 (should not be output to the file)
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example1", "json", schema, context2);
  auto channel = std::move(requireValue(channel_result));
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));

  // Check that it does not contain the message
  std::string content = readFile(path());
  REQUIRE_THAT(content, !ContainsSubstring("Hello, world!"));
}

TEST_CASE_METHOD(McapTestFile, "specify profile") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  options.profile = "test_profile";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example1", "json", schema, context);
  auto& channel = requireValue(channel_result);
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));

  // Check that it contains the profile and library
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("test_profile"));
}

TEST_CASE_METHOD(McapTestFile, "zstd compression") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  options.compression = foxglove::McapCompression::Zstd;
  options.chunk_size = 10000;
  options.use_chunks = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example2", "json", schema, context);
  auto channel = std::move(requireValue(channel_result));
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));

  // Check that it contains the word "zstd"
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("zstd"));
}

TEST_CASE_METHOD(McapTestFile, "lz4 compression") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  options.compression = foxglove::McapCompression::Lz4;
  options.chunk_size = 10000;
  options.use_chunks = true;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write message
  foxglove::Schema schema;
  schema.name = "ExampleSchema";
  auto channel_result = foxglove::RawChannel::create("example3", "json", schema, context);
  auto& channel = requireValue(channel_result);
  std::string data = "Hello, world!";
  channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  auto error = writer->close();
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  // Check if file exists
  REQUIRE(std::filesystem::exists(path()));

  // Check that it contains the word "lz4"
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("lz4"));
}

TEST_CASE_METHOD(McapTestFile, "Channel can outlive Schema") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
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
    // Channel should copy the schema, so this modification has no effect on the output
    data[2] = 'I';
    data[3] = 'L';
    // Use emplace to construct the optional directly
    channel.emplace(std::move(requireValue(result)));
  }

  const std::array<uint8_t, 3> data = {4, 5, 6};
  channel->log(reinterpret_cast<const std::byte*>(data.data()), data.size());

  writer->close();

  REQUIRE(std::filesystem::exists(path()));

  std::string content = readFile(path());
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
  const auto& circle_ts = requireValue(msg.circles[0].timestamp);
  REQUIRE(c_msg.circles[0].timestamp->sec == circle_ts.sec);
  REQUIRE(c_msg.circles[0].timestamp->nsec == circle_ts.nsec);
  const auto& circle_pos = requireValue(msg.circles[0].position);
  REQUIRE(c_msg.circles[0].position->x == circle_pos.x);
  REQUIRE(c_msg.circles[0].position->y == circle_pos.y);
  REQUIRE(c_msg.circles[0].diameter == msg.circles[0].diameter);
  REQUIRE(c_msg.circles[0].thickness == msg.circles[0].thickness);
  const auto& circle_fill = requireValue(msg.circles[0].fill_color);
  REQUIRE(c_msg.circles[0].fill_color->r == circle_fill.r);
  REQUIRE(c_msg.circles[0].fill_color->g == circle_fill.g);
  REQUIRE(c_msg.circles[0].fill_color->b == circle_fill.b);
  REQUIRE(c_msg.circles[0].fill_color->a == circle_fill.a);
  const auto& circle_outline = requireValue(msg.circles[0].outline_color);
  REQUIRE(c_msg.circles[0].outline_color->r == circle_outline.r);
  REQUIRE(c_msg.circles[0].outline_color->g == circle_outline.g);
  REQUIRE(c_msg.circles[0].outline_color->b == circle_outline.b);
  REQUIRE(c_msg.circles[0].outline_color->a == circle_outline.a);

  // Compare points annotation
  const auto& point_ts = requireValue(msg.points[0].timestamp);
  REQUIRE(c_msg.points[0].timestamp->sec == point_ts.sec);
  REQUIRE(c_msg.points[0].timestamp->nsec == point_ts.nsec);
  REQUIRE(static_cast<uint8_t>(c_msg.points[0].type) == static_cast<uint8_t>(msg.points[0].type));
  REQUIRE(c_msg.points[0].points_count == msg.points[0].points.size());
  for (size_t i = 0; i < msg.points[0].points.size(); ++i) {
    REQUIRE(c_msg.points[0].points[i].x == msg.points[0].points[i].x);
    REQUIRE(c_msg.points[0].points[i].y == msg.points[0].points[i].y);
  }
  const auto& point_outline = requireValue(msg.points[0].outline_color);
  REQUIRE(c_msg.points[0].outline_color->r == point_outline.r);
  REQUIRE(c_msg.points[0].outline_color->g == point_outline.g);
  REQUIRE(c_msg.points[0].outline_color->b == point_outline.b);
  REQUIRE(c_msg.points[0].outline_color->a == point_outline.a);
  REQUIRE(c_msg.points[0].outline_colors_count == msg.points[0].outline_colors.size());
  for (size_t i = 0; i < msg.points[0].outline_colors.size(); ++i) {
    REQUIRE(c_msg.points[0].outline_colors[i].r == msg.points[0].outline_colors[i].r);
    REQUIRE(c_msg.points[0].outline_colors[i].g == msg.points[0].outline_colors[i].g);
    REQUIRE(c_msg.points[0].outline_colors[i].b == msg.points[0].outline_colors[i].b);
    REQUIRE(c_msg.points[0].outline_colors[i].a == msg.points[0].outline_colors[i].a);
  }
  const auto& point_fill = requireValue(msg.points[0].fill_color);
  REQUIRE(c_msg.points[0].fill_color->r == point_fill.r);
  REQUIRE(c_msg.points[0].fill_color->g == point_fill.g);
  REQUIRE(c_msg.points[0].fill_color->b == point_fill.b);
  REQUIRE(c_msg.points[0].fill_color->a == point_fill.a);
  REQUIRE(c_msg.points[0].thickness == msg.points[0].thickness);

  // Compare text annotation
  const auto& text_ts = requireValue(msg.texts[0].timestamp);
  REQUIRE(c_msg.texts[0].timestamp->sec == text_ts.sec);
  REQUIRE(c_msg.texts[0].timestamp->nsec == text_ts.nsec);
  const auto& text_pos = requireValue(msg.texts[0].position);
  REQUIRE(c_msg.texts[0].position->x == text_pos.x);
  REQUIRE(c_msg.texts[0].position->y == text_pos.y);
  REQUIRE(c_msg.texts[0].text.data == msg.texts[0].text.data());
  REQUIRE(c_msg.texts[0].text.len == msg.texts[0].text.size());
  REQUIRE(c_msg.texts[0].font_size == msg.texts[0].font_size);
  const auto& text_color = requireValue(msg.texts[0].text_color);
  REQUIRE(c_msg.texts[0].text_color->r == text_color.r);
  REQUIRE(c_msg.texts[0].text_color->g == text_color.g);
  REQUIRE(c_msg.texts[0].text_color->b == text_color.b);
  REQUIRE(c_msg.texts[0].text_color->a == text_color.a);
  const auto& bg_color = requireValue(msg.texts[0].background_color);
  REQUIRE(c_msg.texts[0].background_color->r == bg_color.r);
  REQUIRE(c_msg.texts[0].background_color->g == bg_color.g);
  REQUIRE(c_msg.texts[0].background_color->b == bg_color.b);
  REQUIRE(c_msg.texts[0].background_color->a == bg_color.a);
}

TEST_CASE_METHOD(McapTestFile, "ImageAnnotations channel") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  options.compression = foxglove::McapCompression::None;
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  auto channel_result = foxglove::schemas::ImageAnnotationsChannel::create("example", context);
  auto channel = std::move(requireValue(channel_result));

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

  REQUIRE(std::filesystem::exists(path()));

  // Check that the file contains our annotations
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("Sample text"));
  REQUIRE_THAT(content, ContainsSubstring("ImageAnnotations"));
}

TEST_CASE("MCAP Channel filtering") {
  auto suffix = std::to_string(std::random_device{}());
  FileCleanup file_1("test_filter_" + suffix + "-1.mcap");
  FileCleanup file_2("test_filter_" + suffix + "-2.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions opts_1;
  opts_1.context = context;
  opts_1.compression = foxglove::McapCompression::None;
  opts_1.path = file_1.path();
  opts_1.sink_channel_filter = [](const foxglove::ChannelDescriptor& channel) -> bool {
    return channel.topic() == "/1";
  };
  auto writer_res_1 = foxglove::McapWriter::create(opts_1);
  if (!writer_res_1.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_res_1.error()) << '\n';
  }
  auto writer_1 = std::move(requireValue(writer_res_1));

  foxglove::McapWriterOptions opts_2;
  opts_2.context = context;
  opts_2.compression = foxglove::McapCompression::None;
  opts_2.path = file_2.path();
  opts_2.sink_channel_filter = [](const foxglove::ChannelDescriptor& channel) -> bool {
    // Only log to topic /2, and validate the schema while we're at it
    if (channel.topic() == "/2") {
      auto schema = channel.schema();
      REQUIRE(requireValue(schema).name == "Topic2Schema");
      REQUIRE(requireValue(schema).encoding == "fake-encoding");
      auto metadata = channel.metadata();
      REQUIRE(requireValue(metadata).size() == 2);
      REQUIRE(requireValue(metadata).at("key1") == "value1");
      REQUIRE(requireValue(metadata).at("key2") == "value2");
      return true;
    }
    return false;
  };
  auto writer_res_2 = foxglove::McapWriter::create(opts_2);
  auto writer_2 = std::move(requireValue(writer_res_2));

  {
    auto result = foxglove::RawChannel::create("/1", "json", std::nullopt, context);
    auto channel = std::move(requireValue(result));
    std::string data = "Topic 1 msg";
    channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());
  }
  {
    foxglove::Schema topic2_schema;
    topic2_schema.name = "Topic2Schema";
    topic2_schema.encoding = "fake-encoding";
    std::string schema_data = "FAKESCHEMA";
    topic2_schema.data = reinterpret_cast<const std::byte*>(schema_data.data());
    topic2_schema.data_len = schema_data.size();

    std::map<std::string, std::string> metadata = {{"key1", "value1"}, {"key2", "value2"}};

    auto result =
      foxglove::RawChannel::create("/2", "json", std::move(topic2_schema), context, metadata);
    auto channel = std::move(requireValue(result));
    std::string data = "Topic 2 msg";
    channel.log(reinterpret_cast<const std::byte*>(data.data()), data.size());
  }

  writer_1.close();
  writer_2.close();

  // Check that the file contains the correct filtered messages
  std::string content = readFile(file_1.path());
  REQUIRE_THAT(content, ContainsSubstring("Topic 1 msg"));
  REQUIRE_THAT(content, !ContainsSubstring("Topic 2 msg"));

  content = readFile(file_2.path());
  REQUIRE_THAT(content, !ContainsSubstring("Topic 1 msg"));
  REQUIRE_THAT(content, ContainsSubstring("Topic 2 msg"));
}

TEST_CASE_METHOD(McapTestFile, "Write metadata records to MCAP") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write first metadata record
  std::map<std::string, std::string> metadata1 = {{"key1", "value1"}, {"key2", "value2"}};
  auto error1 = writer->writeMetadata("metadata_record_1", metadata1.begin(), metadata1.end());
  REQUIRE(error1 == foxglove::FoxgloveError::Ok);

  // Write second metadata record
  std::map<std::string, std::string> metadata2 = {{"key3", "value3"}, {"key4", "value4"}};
  auto error2 = writer->writeMetadata("metadata_record_2", metadata2.begin(), metadata2.end());
  REQUIRE(error2 == foxglove::FoxgloveError::Ok);

  writer->close();

  REQUIRE(std::filesystem::exists(path()));

  // Verify both metadata records were written
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("metadata_record_1"));
  REQUIRE_THAT(content, ContainsSubstring("key1"));
  REQUIRE_THAT(content, ContainsSubstring("value1"));
  REQUIRE_THAT(content, ContainsSubstring("key2"));
  REQUIRE_THAT(content, ContainsSubstring("value2"));
  REQUIRE_THAT(content, ContainsSubstring("metadata_record_2"));
  REQUIRE_THAT(content, ContainsSubstring("key3"));
  REQUIRE_THAT(content, ContainsSubstring("value3"));
  REQUIRE_THAT(content, ContainsSubstring("key4"));
  REQUIRE_THAT(content, ContainsSubstring("value4"));
}

TEST_CASE_METHOD(McapTestFile, "Write empty metadata") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write empty metadata (should do nothing according to documentation)
  std::map<std::string, std::string> metadata;
  auto error = writer->writeMetadata("empty_metadata", metadata.begin(), metadata.end());
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  writer->close();

  REQUIRE(std::filesystem::exists(path()));

  std::string content = readFile(path());
  REQUIRE_THAT(content, !ContainsSubstring("empty_metadata"));
}

TEST_CASE("Custom writer basic functionality") {
  auto context = foxglove::Context::create();

  bool write_called = false;
  bool flush_called = false;
  bool seek_called = false;
  size_t cursor = 0;
  std::vector<uint8_t> buffer;
  foxglove::CustomWriter custom_writer;
  custom_writer.write =
    [&buffer, &write_called, &cursor](const uint8_t* data, size_t len, int* error) -> size_t {
    write_called = true;
    *error = 0;
    if (cursor != buffer.size()) {
      REQUIRE(cursor + len < buffer.size());
      std::memcpy(buffer.data() + cursor, data, len);
      cursor += len;
      return len;
    }
    buffer.insert(buffer.end(), data, data + len);
    cursor += len;
    return len;
  };
  custom_writer.flush = [&flush_called]() -> int {
    flush_called = true;
    return 0;
  };
  custom_writer.seek =
    [&cursor, &buffer, &seek_called](int64_t pos, int whence, uint64_t* new_pos) -> int {
    seek_called = true;
    switch (whence) {
      case SEEK_SET:
        cursor = pos;
        break;
      case SEEK_CUR:
        cursor += pos;
        break;
      case SEEK_END:
        cursor = buffer.size() + pos;
        break;
      default:
        assert(false);
    }
    *new_pos = cursor;
    return 0;
  };

  foxglove::McapWriterOptions options;
  options.custom_writer = custom_writer;
  options.context = context;

  auto custom_mcap = foxglove::McapWriter::create(options);

  auto channel_result = foxglove::schemas::Point2Channel::create("test_topic", context);
  auto channel = std::move(requireValue(channel_result));
  channel.log(foxglove::schemas::Point2{1.0, 2.0});
  channel.log(foxglove::schemas::Point2{3.0, 4.0});
  channel.close();
  requireValue(custom_mcap).close();

  // Verify callbacks were called
  REQUIRE(write_called);
  REQUIRE(flush_called);
  REQUIRE(seek_called);
  // Verify MCAP data was written
  std::string custom_content = std::string(buffer.begin(), buffer.end());
  REQUIRE_THAT(custom_content, ContainsSubstring("Point2"));
}

TEST_CASE_METHOD(McapTestFile, "Write single attachment to MCAP") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write an attachment
  std::string attachment_data = R"({"setting": true})";
  foxglove::Attachment attachment;
  attachment.log_time = 1000000000;
  attachment.create_time = 900000000;
  attachment.name = "config.json";
  attachment.media_type = "application/json";
  attachment.data = reinterpret_cast<const std::byte*>(attachment_data.data());
  attachment.data_len = attachment_data.size();

  auto error = writer->attach(attachment);
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  writer->close();

  REQUIRE(std::filesystem::exists(path()));

  // Verify the attachment was written
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("config.json"));
  REQUIRE_THAT(content, ContainsSubstring("application/json"));
  REQUIRE_THAT(content, ContainsSubstring(R"({"setting": true})"));
}

TEST_CASE_METHOD(McapTestFile, "Write multiple attachments to MCAP") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write first attachment
  std::string config_data = R"({"debug": false})";
  foxglove::Attachment config_attachment;
  config_attachment.log_time = 1000000000;
  config_attachment.create_time = 900000000;
  config_attachment.name = "config.yaml";
  config_attachment.media_type = "application/yaml";
  config_attachment.data = reinterpret_cast<const std::byte*>(config_data.data());
  config_attachment.data_len = config_data.size();

  auto error1 = writer->attach(config_attachment);
  REQUIRE(error1 == foxglove::FoxgloveError::Ok);

  // Write second attachment
  std::string calibration_data = "calibration binary data here";
  foxglove::Attachment calibration_attachment;
  calibration_attachment.log_time = 2000000000;
  calibration_attachment.create_time = 1800000000;
  calibration_attachment.name = "calibration.bin";
  calibration_attachment.media_type = "application/octet-stream";
  calibration_attachment.data = reinterpret_cast<const std::byte*>(calibration_data.data());
  calibration_attachment.data_len = calibration_data.size();

  auto error2 = writer->attach(calibration_attachment);
  REQUIRE(error2 == foxglove::FoxgloveError::Ok);

  writer->close();

  REQUIRE(std::filesystem::exists(path()));

  // Verify both attachments were written
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("config.yaml"));
  REQUIRE_THAT(content, ContainsSubstring("calibration.bin"));
  REQUIRE_THAT(content, ContainsSubstring("calibration binary data here"));
}

TEST_CASE_METHOD(McapTestFile, "Write empty attachment data") {
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = path();
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write an attachment with empty data
  foxglove::Attachment attachment;
  attachment.log_time = 1000000000;
  attachment.create_time = 900000000;
  attachment.name = "empty.txt";
  attachment.media_type = "text/plain";
  attachment.data = nullptr;
  attachment.data_len = 0;

  auto error = writer->attach(attachment);
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  writer->close();

  REQUIRE(std::filesystem::exists(path()));

  // Verify the attachment name was written
  std::string content = readFile(path());
  REQUIRE_THAT(content, ContainsSubstring("empty.txt"));
}

TEST_CASE("McapWriterOptions defaults match C defaults") {
  foxglove::McapWriterOptions defaults;
  auto c = foxglove_mcap_options_default();
  auto converted = foxglove::to_c_mcap_options(defaults);

  CHECK(converted.chunk_size == c.chunk_size);
  CHECK(converted.compression == c.compression);
  CHECK(converted.use_chunks == c.use_chunks);
  CHECK(converted.disable_seeking == c.disable_seeking);
  CHECK(converted.emit_statistics == c.emit_statistics);
  CHECK(converted.emit_summary_offsets == c.emit_summary_offsets);
  CHECK(converted.emit_message_indexes == c.emit_message_indexes);
  CHECK(converted.emit_chunk_indexes == c.emit_chunk_indexes);
  CHECK(converted.emit_attachment_indexes == c.emit_attachment_indexes);
  CHECK(converted.emit_metadata_indexes == c.emit_metadata_indexes);
  CHECK(converted.repeat_channels == c.repeat_channels);
  CHECK(converted.repeat_schemas == c.repeat_schemas);
  CHECK(converted.calculate_chunk_crcs == c.calculate_chunk_crcs);
  CHECK(converted.calculate_data_section_crc == c.calculate_data_section_crc);
  CHECK(converted.calculate_summary_section_crc == c.calculate_summary_section_crc);
  CHECK(converted.calculate_attachment_crcs == c.calculate_attachment_crcs);
  CHECK(converted.compression_level == c.compression_level);
  CHECK(converted.compression_threads == c.compression_threads);
  CHECK(converted.truncate == c.truncate);
}
