#include <foxglove/channel.hpp>
#include <foxglove/foxglove.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/schemas.hpp>

#include <google/protobuf/descriptor.pb.h>

#include <cstdlib>
#include <filesystem>
#include <string>

#include "protos/fruit.pb.h"

int main(int argc, const char* argv[]) {
  foxglove::setLogLevel(foxglove::LogLevel::Debug);

  // Make it easy to override the path when running in a container
  const char* output_path = std::getenv("MCAP_OUTPUT_PATH");
  if (!output_path) {
    output_path = "example-custom-protobuf.mcap";
  }

  foxglove::McapWriterOptions mcap_options = {};
  mcap_options.path = output_path;
  auto writer_result = foxglove::McapWriter::create(mcap_options);
  if (!writer_result.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_result.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writer_result.value());

  auto descriptor = fruit::Apple::descriptor();

  // Create a schema for the Apple message
  foxglove::Schema schema;
  schema.encoding = "protobuf";
  schema.name = descriptor->full_name();

  // Create a FileDescriptorSet containing our message descriptor
  google::protobuf::FileDescriptorSet file_descriptor_set;
  const google::protobuf::FileDescriptor* file_descriptor = descriptor->file();
  file_descriptor->CopyTo(file_descriptor_set.add_file());

  std::string serialized_descriptor = file_descriptor_set.SerializeAsString();
  schema.data = reinterpret_cast<const std::byte*>(serialized_descriptor.data());
  schema.data_len = serialized_descriptor.size();

  auto channel_result = foxglove::RawChannel::create("/apple", "protobuf", std::move(schema));
  if (!channel_result.has_value()) {
    std::cerr << "Failed to create channel: " << foxglove::strerror(channel_result.error()) << '\n';
    return 1;
  }
  auto apple_channel = std::move(channel_result.value());

  // Create an Apple message, serialize it, and log it to the channel
  fruit::Apple apple;
  apple.set_color("red");
  apple.set_diameter(10);
  std::string apple_data = apple.SerializeAsString();
  apple_channel.log(reinterpret_cast<const std::byte*>(apple_data.data()), apple_data.size());

  return 0;
}
