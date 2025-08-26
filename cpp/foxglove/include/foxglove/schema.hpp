#pragma once

#include <memory>
#include <string>

namespace foxglove {

/// @brief A description of the data format of messages in a channel.
///
/// It allows Foxglove to validate messages and provide richer visualizations.
struct Schema {
  /// @brief An identifier for the schema.
  std::string name;
  /// @brief The encoding of the schema data. For example "jsonschema" or "protobuf".
  ///
  /// The [well-known schema encodings] are preferred.
  ///
  /// [well-known schema encodings]: https://mcap.dev/spec/registry#well-known-schema-encodings
  std::string encoding;
  /// @brief Must conform to the schema encoding. If encoding is an empty string, data should be 0
  /// length.
  const std::byte* data = nullptr;
  /// @brief The length of the schema data.
  size_t data_len = 0;
};

}  // namespace foxglove
