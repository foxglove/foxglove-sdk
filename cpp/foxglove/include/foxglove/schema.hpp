#pragma once

#include <string>

namespace foxglove {

/// A Schema is a description of the data format of messages in a channel.
///
/// It allows Foxglove to validate messages and provide richer visualizations.
/// See the [MCAP spec](https://mcap.dev/spec#schema-op0x03) for more information.
struct FoxgloveSchema {
  /// An identifier for the schema.
  std::string_view name;
  /// The encoding of the schema data.
  /// [well-known schema encodings]: https://mcap.dev/spec/registry#well-known-schema-encodings
  std::string_view encoding;
  /// The schema data.
  const uint8_t* data;
  /// The length of the schema data.
  size_t data_len;
};

}  // namespace foxglove
