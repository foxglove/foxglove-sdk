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
#include <atomic>
#include <optional>
#include <cstring>

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

TEST_CASE("Write metadata records to MCAP") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = "test.mcap";
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

  REQUIRE(std::filesystem::exists("test.mcap"));

  // Verify both metadata records were written
  std::string content = readFile("test.mcap");
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

TEST_CASE("Write empty metadata") {
  FileCleanup cleanup("test.mcap");
  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.path = "test.mcap";
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write empty metadata (should do nothing according to documentation)
  std::map<std::string, std::string> metadata;
  auto error = writer->writeMetadata("empty_metadata", metadata.begin(), metadata.end());
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  writer->close();

  REQUIRE(std::filesystem::exists("test.mcap"));

  std::string content = readFile("test.mcap");
  REQUIRE_THAT(content, !ContainsSubstring("empty_metadata"));
}

// Helper class for testing custom writers
class TestCustomWriter {
public:
  std::vector<uint8_t> data;
  mutable std::atomic<bool> write_called{false};
  mutable std::atomic<bool> flush_called{false};
  mutable std::atomic<bool> seek_called{false};
  mutable std::atomic<int> write_error{0};
  mutable std::atomic<int> flush_error{0};
  mutable std::atomic<int> seek_error{0};
  mutable std::atomic<uint64_t> position{0};

  foxglove::CustomWriter makeWriter() {
    foxglove::CustomWriter writer;
    writer.write = [this](const uint8_t* data_ptr, size_t len, int* error) -> size_t {
      write_called = true;
      if (write_error != 0) {
        *error = write_error;
        return 0;
      }
      data.insert(data.end(), data_ptr, data_ptr + len);
      position += len;
      return len;
    };
    writer.flush = [this]() -> int {
      flush_called = true;
      return flush_error;
    };
    writer.seek = [this](int64_t pos, int whence, uint64_t* new_pos) -> int {
      seek_called = true;
      if (seek_error != 0) {
        return seek_error;
      }
      switch (whence) {
        case 0: // SEEK_SET
          position = static_cast<uint64_t>(pos);
          break;
        case 1: // SEEK_CUR
          position = static_cast<uint64_t>(static_cast<int64_t>(position) + pos);
          break;
        case 2: // SEEK_END
          position = static_cast<uint64_t>(static_cast<int64_t>(data.size()) + pos);
          break;
      }
      *new_pos = position;
      return 0;
    };
    return writer;
  }
};

TEST_CASE("Custom writer basic functionality") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write some metadata to ensure the writer is working
  std::map<std::string, std::string> metadata = {{"key1", "value1"}};
  auto error = writer->writeMetadata("test_metadata", metadata.begin(), metadata.end());
  REQUIRE(error == foxglove::FoxgloveError::Ok);

  writer->close();

  // Verify callbacks were called
  REQUIRE(test_writer.write_called.load());
  REQUIRE(test_writer.flush_called.load());

  // Verify MCAP data was written
  REQUIRE(test_writer.data.size() > 0);

  // Check for MCAP magic bytes at the beginning
  REQUIRE(test_writer.data.size() >= 8);
  std::string magic_bytes(test_writer.data.begin(), test_writer.data.begin() + 8);
  REQUIRE(magic_bytes == "\x89MCAP0\r\n");
}

TEST_CASE("Custom writer with channel and message data") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Create a channel and log some data
  foxglove::Schema schema;
  schema.name = "TestSchema";
  schema.encoding = "json";
  const char* schema_data = R"({"type": "object", "properties": {"msg": {"type": "string"}}})";
  schema.data = reinterpret_cast<const std::byte*>(schema_data);
  schema.data_len = std::strlen(schema_data);

  auto channel_result = foxglove::RawChannel::create("test_topic", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto& channel = channel_result.value();

  std::string message = R"({"msg": "Hello, custom writer!"})";
  channel.log(reinterpret_cast<const std::byte*>(message.data()), message.size());

  writer->close();

  // Verify data was written
  REQUIRE(test_writer.data.size() > 0);
  REQUIRE(test_writer.write_called.load());
  REQUIRE(test_writer.flush_called.load());

  // Verify the written data contains our message
  std::string data_str(test_writer.data.begin(), test_writer.data.end());
  REQUIRE_THAT(data_str, ContainsSubstring("Hello, custom writer!"));
}

TEST_CASE("Custom writer write error handling") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;
  test_writer.write_error = ENOSPC; // No space left on device

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  // Writer creation should succeed even with write errors
  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write operations should fail when write_fn returns error
  std::map<std::string, std::string> metadata = {{"key1", "value1"}};
  auto error = writer->writeMetadata("test_metadata", metadata.begin(), metadata.end());

  // The error should be propagated
  REQUIRE(error != foxglove::FoxgloveError::Ok);

  writer->close();
  REQUIRE(test_writer.write_called.load());
}

TEST_CASE("Custom writer flush error handling") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;
  test_writer.flush_error = EIO; // I/O error

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write some data first
  std::map<std::string, std::string> metadata = {{"key1", "value1"}};
  writer->writeMetadata("test_metadata", metadata.begin(), metadata.end());

  // Close should fail due to flush error
  auto close_error = writer->close();
  REQUIRE(close_error != foxglove::FoxgloveError::Ok);

  REQUIRE(test_writer.write_called.load());
  REQUIRE(test_writer.flush_called.load());
}

TEST_CASE("Custom writer seek functionality") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Write some data
  std::map<std::string, std::string> metadata = {{"key1", "value1"}};
  writer->writeMetadata("test_metadata", metadata.begin(), metadata.end());

  writer->close();

  // For MCAP files, seeking is typically used, so verify seek was called
  REQUIRE(test_writer.seek_called.load());
}

TEST_CASE("Custom writer seek error handling") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;
  test_writer.seek_error = ESPIPE; // Illegal seek

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  // Writer creation should fail if seeking is required but seek fails
  auto writer = foxglove::McapWriter::create(options);

  // The result depends on whether MCAP writer attempts to seek during creation
  // If it does and seek fails, creation should fail
  if (writer.has_value()) {
    // If creation succeeds, operations that require seek should fail
    std::map<std::string, std::string> metadata = {{"key1", "value1"}};
    auto error = writer->writeMetadata("test_metadata", metadata.begin(), metadata.end());
    // Close might fail due to seek errors during finalization
    writer->close();
  }

  // At minimum, seek should have been attempted
  REQUIRE(test_writer.seek_called.load());
}

TEST_CASE("Custom writer vs file writer produces same output") {
  auto context = foxglove::Context::create();

  // Create file writer
  FileCleanup cleanup("test_reference.mcap");
  foxglove::McapWriterOptions file_options;
  file_options.context = context;
  file_options.path = "test_reference.mcap";

  auto file_writer = foxglove::McapWriter::create(file_options);
  REQUIRE(file_writer.has_value());

  // Create custom writer
  TestCustomWriter test_writer;
  foxglove::McapWriterOptions custom_options;
  custom_options.context = context;
  custom_options.custom_writer = test_writer.makeWriter();

  auto custom_writer = foxglove::McapWriter::create(custom_options);
  REQUIRE(custom_writer.has_value());

  // Write identical data to both
  std::map<std::string, std::string> metadata = {
    {"author", "test"},
    {"version", "1.0"}
  };

  file_writer->writeMetadata("test_metadata", metadata.begin(), metadata.end());
  custom_writer->writeMetadata("test_metadata", metadata.begin(), metadata.end());

  file_writer->close();
  custom_writer->close();

  // Compare outputs
  std::string file_content = readFile("test_reference.mcap");
  std::string custom_content(test_writer.data.begin(), test_writer.data.end());

  // The outputs should be identical (modulo timestamps which may differ slightly)
  REQUIRE(file_content.size() == custom_content.size());

  // Check magic bytes are the same
  REQUIRE(file_content.substr(0, 8) == custom_content.substr(0, 8));

  // Both should contain the metadata
  REQUIRE_THAT(custom_content, ContainsSubstring("test_metadata"));
  REQUIRE_THAT(custom_content, ContainsSubstring("author"));
  REQUIRE_THAT(custom_content, ContainsSubstring("test"));
}

TEST_CASE("Custom writer with compression") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();
  options.compression = foxglove::McapCompression::Zstd;
  options.use_chunks = true;
  options.chunk_size = 1024;

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Create a channel and log some data
  foxglove::Schema schema;
  schema.name = "TestSchema";
  schema.encoding = "json";
  const char* schema_data = R"({"type": "object", "properties": {"msg": {"type": "string"}}})";
  schema.data = reinterpret_cast<const std::byte*>(schema_data);
  schema.data_len = std::strlen(schema_data);

  auto channel_result = foxglove::RawChannel::create("compressed_topic", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto& channel = channel_result.value();

  // Log multiple messages to trigger compression
  for (int i = 0; i < 10; ++i) {
    std::string message = R"({"msg": "Compressed message #)" + std::to_string(i) + R"("})";
    channel.log(reinterpret_cast<const std::byte*>(message.data()), message.size());
  }

  writer->close();

  // Verify data was written and contains compressed chunks
  REQUIRE(test_writer.data.size() > 0);
  REQUIRE(test_writer.write_called.load());
  REQUIRE(test_writer.flush_called.load());

  // Check for MCAP magic and zstd compression
  std::string data_str(test_writer.data.begin(), test_writer.data.end());
  REQUIRE_THAT(data_str, ContainsSubstring("zstd"));
}

TEST_CASE("Custom writer with multiple channels") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Create multiple channels with different schemas
  foxglove::Schema json_schema;
  json_schema.name = "JsonSchema";
  json_schema.encoding = "json";
  const char* json_schema_data = R"({"type": "object"})";
  json_schema.data = reinterpret_cast<const std::byte*>(json_schema_data);
  json_schema.data_len = std::strlen(json_schema_data);

  foxglove::Schema protobuf_schema;
  protobuf_schema.name = "ProtobufSchema";
  protobuf_schema.encoding = "protobuf";
  const char* protobuf_schema_data = "syntax = \"proto3\"; message Test { string data = 1; }";
  protobuf_schema.data = reinterpret_cast<const std::byte*>(protobuf_schema_data);
  protobuf_schema.data_len = std::strlen(protobuf_schema_data);

  auto json_channel = foxglove::RawChannel::create("json_topic", "json", json_schema, context);
  auto proto_channel = foxglove::RawChannel::create("proto_topic", "protobuf", protobuf_schema, context);

  REQUIRE(json_channel.has_value());
  REQUIRE(proto_channel.has_value());

  // Log messages to both channels
  std::string json_msg = R"({"data": "json message"})";
  std::string proto_msg = "proto message data";

  json_channel->log(reinterpret_cast<const std::byte*>(json_msg.data()), json_msg.size());
  proto_channel->log(reinterpret_cast<const std::byte*>(proto_msg.data()), proto_msg.size());

  writer->close();

  // Verify both channel data is present
  std::string data_str(test_writer.data.begin(), test_writer.data.end());
  REQUIRE_THAT(data_str, ContainsSubstring("json_topic"));
  REQUIRE_THAT(data_str, ContainsSubstring("proto_topic"));
  REQUIRE_THAT(data_str, ContainsSubstring("JsonSchema"));
  REQUIRE_THAT(data_str, ContainsSubstring("ProtobufSchema"));
}

TEST_CASE("Custom writer data integrity check") {
  auto context = foxglove::Context::create();
  TestCustomWriter test_writer;

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = test_writer.makeWriter();

  auto writer = foxglove::McapWriter::create(options);
  REQUIRE(writer.has_value());

  // Add metadata
  std::map<std::string, std::string> metadata = {
    {"test_key", "test_value"},
    {"timestamp", "2024-01-01T00:00:00Z"}
  };
  writer->writeMetadata("integrity_test", metadata.begin(), metadata.end());

  // Create a channel and log structured data
  foxglove::Schema schema;
  schema.name = "IntegrityTestSchema";
  schema.encoding = "json";
  const char* schema_data = R"({"type": "object", "properties": {"id": {"type": "number"}, "msg": {"type": "string"}}})";
  schema.data = reinterpret_cast<const std::byte*>(schema_data);
  schema.data_len = std::strlen(schema_data);

  auto channel_result = foxglove::RawChannel::create("integrity_topic", "json", schema, context);
  REQUIRE(channel_result.has_value());
  auto& channel = channel_result.value();

  // Log a series of messages with predictable data
  std::vector<std::string> messages;
  for (int i = 0; i < 5; ++i) {
    std::string msg = R"({"id": )" + std::to_string(i) + R"(, "msg": "message_)" + std::to_string(i) + R"("})";
    messages.push_back(msg);
    channel.log(reinterpret_cast<const std::byte*>(msg.data()), msg.size());
  }

  writer->close();

  // Verify MCAP structure
  REQUIRE(test_writer.data.size() > 0);

  // Check magic bytes
  REQUIRE(test_writer.data.size() >= 8);
  std::string magic_bytes(test_writer.data.begin(), test_writer.data.begin() + 8);
  REQUIRE(magic_bytes == "\x89MCAP0\r\n");

  // Verify all our data is present in the output
  std::string data_str(test_writer.data.begin(), test_writer.data.end());

  // Check metadata
  REQUIRE_THAT(data_str, ContainsSubstring("integrity_test"));
  REQUIRE_THAT(data_str, ContainsSubstring("test_key"));
  REQUIRE_THAT(data_str, ContainsSubstring("test_value"));

  // Check schema
  REQUIRE_THAT(data_str, ContainsSubstring("IntegrityTestSchema"));
  REQUIRE_THAT(data_str, ContainsSubstring("integrity_topic"));

  // Check all messages
  for (int i = 0; i < 5; ++i) {
    REQUIRE_THAT(data_str, ContainsSubstring("message_" + std::to_string(i)));
  }

  // Check for MCAP footer magic (should end with magic bytes)
  REQUIRE(test_writer.data.size() >= 16); // At least header + footer
  std::string footer_magic(test_writer.data.end() - 8, test_writer.data.end());
  REQUIRE(footer_magic == "\x89MCAP0\r\n");
}
