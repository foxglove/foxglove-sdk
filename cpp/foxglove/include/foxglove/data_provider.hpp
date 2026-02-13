#pragma once

/// @file
/// Types and utilities for building remote data loader manifests.
///
/// Use @ref foxglove::data_provider::ChannelSet to declare channels, then construct a
/// @ref foxglove::data_provider::StreamedSource with the resulting topics and schemas.
///
/// @note This header requires [nlohmann/json](https://github.com/nlohmann/json) to be available
/// on the include path.
///
/// ## Example
///
/// @code{.cpp}
/// #include <foxglove/data_provider.hpp>
/// #include <foxglove/schemas.hpp>
///
/// namespace dp = foxglove::data_provider;
///
/// dp::ChannelSet channels;
/// channels.insert<foxglove::schemas::Vector3>("/demo");
///
/// dp::StreamedSource source;
/// source.url = "/v1/data?flightId=ABC123";
/// source.id = "flight-v1-ABC123";
/// source.topics = std::move(channels.topics);
/// source.schemas = std::move(channels.schemas);
/// source.start_time = "2024-01-01T00:00:00Z";
/// source.end_time = "2024-01-02T00:00:00Z";
///
/// dp::Manifest manifest;
/// manifest.name = "Flight ABC123";
/// manifest.sources = {std::move(source)};
///
/// nlohmann::json j = manifest;
/// std::string json_str = j.dump();
/// @endcode

#include <foxglove/schema.hpp>

#include <nlohmann/json.hpp>

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

/// The foxglove namespace.
namespace foxglove::data_provider {

// ============================================================================
// Base64 encoding
// ============================================================================

/// @brief Base64-encode binary data.
///
/// This is provided for encoding schema data in manifest responses. The returned
/// string uses the standard base64 alphabet with '=' padding.
///
/// @param data Pointer to the data to encode.
/// @param len Number of bytes to encode.
/// @return The base64-encoded string.
inline std::string base64_encode(const std::byte* data, size_t len) {
  static constexpr const char CHARS[] =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const auto* bytes = reinterpret_cast<const uint8_t*>(data);
  std::string result;
  result.reserve((len + 2) / 3 * 4);
  for (size_t i = 0; i < len; i += 3) {
    uint32_t n = static_cast<uint32_t>(bytes[i]) << 16;
    if (i + 1 < len) {
      n |= static_cast<uint32_t>(bytes[i + 1]) << 8;
    }
    if (i + 2 < len) {
      n |= static_cast<uint32_t>(bytes[i + 2]);
    }
    result += CHARS[(n >> 18) & 0x3F];
    result += CHARS[(n >> 12) & 0x3F];
    result += (i + 1 < len) ? CHARS[(n >> 6) & 0x3F] : '=';
    result += (i + 2 < len) ? CHARS[n & 0x3F] : '=';
  }
  return result;
}

// ============================================================================
// Manifest types
// ============================================================================

/// @brief A topic in a streamed source.
struct Topic {
  /// @brief Topic name.
  std::string name;
  /// @brief Message encoding (e.g. "protobuf").
  std::string message_encoding;
  /// @brief Schema ID, if this topic has an associated schema.
  std::optional<uint16_t> schema_id;
};

/// @brief A schema in a streamed source.
///
/// Schema data is stored as a base64-encoded string, matching the JSON wire format.
struct Schema {
  /// @brief Unique schema ID within this source.
  uint16_t id = 0;
  /// @brief Schema name.
  std::string name;
  /// @brief Schema encoding (e.g. "protobuf").
  std::string encoding;
  /// @brief Raw schema data, base64-encoded.
  std::string data;
};

/// @brief A streamed (non-seekable) data source.
///
/// Represents a URL data source that must be read sequentially. The client will
/// fetch the URL and read the response body as a stream of MCAP bytes.
struct StreamedSource {
  /// @brief URL to fetch the data from. Can be absolute or relative.
  /// If `id` is absent, this must uniquely identify the data.
  std::string url;
  /// @brief Identifier for the data source. If present, must be unique.
  /// If absent, the URL is used as the identifier.
  std::optional<std::string> id;
  /// @brief Topics present in the data.
  std::vector<Topic> topics;
  /// @brief Schemas present in the data.
  std::vector<Schema> schemas;
  /// @brief Earliest timestamp of any message in the data source (ISO 8601).
  ///
  /// You can provide a lower bound if this is not known exactly. This determines the
  /// start time of the seek bar in the Foxglove app.
  std::string start_time;
  /// @brief Latest timestamp of any message in the data (ISO 8601).
  std::string end_time;
};

/// @brief Manifest of upstream sources returned by the manifest endpoint.
struct Manifest {
  /// @brief Human-readable display name for this manifest.
  std::optional<std::string> name;
  /// @brief Data sources in this manifest.
  std::vector<StreamedSource> sources;
};

// ============================================================================
// JSON serialization (nlohmann/json)
// ============================================================================

/// @cond foxglove_internal
inline void to_json(nlohmann::json& j, const Topic& t) {
  j = nlohmann::json{
    {"name", t.name},
    {"messageEncoding", t.message_encoding},
  };
  if (t.schema_id.has_value()) {
    j["schemaId"] = *t.schema_id;
  }
}

inline void to_json(nlohmann::json& j, const Schema& s) {
  j = nlohmann::json{
    {"id", s.id},
    {"name", s.name},
    {"encoding", s.encoding},
    {"data", s.data},
  };
}

inline void to_json(nlohmann::json& j, const StreamedSource& s) {
  j = nlohmann::json{
    {"url", s.url},
    {"topics", s.topics},
    {"schemas", s.schemas},
    {"startTime", s.start_time},
    {"endTime", s.end_time},
  };
  if (s.id.has_value()) {
    j["id"] = *s.id;
  }
}

inline void to_json(nlohmann::json& j, const Manifest& m) {
  j = nlohmann::json{
    {"sources", m.sources},
  };
  if (m.name.has_value()) {
    j["name"] = *m.name;
  }
}
/// @endcond

// ============================================================================
// ChannelSet
// ============================================================================

/// @brief A helper for building topic and schema metadata for a @ref StreamedSource.
///
/// Handles schema extraction from Foxglove schema types, schema ID assignment,
/// and deduplication. If multiple channels share the same schema, only one schema
/// entry will be created.
///
/// @code{.cpp}
/// foxglove::data_provider::ChannelSet channels;
/// channels.insert<foxglove::schemas::Vector3>("/topic1");
/// channels.insert<foxglove::schemas::Vector3>("/topic2"); // reuses schema ID
///
/// foxglove::data_provider::StreamedSource source;
/// source.topics = std::move(channels.topics);
/// source.schemas = std::move(channels.schemas);
/// @endcode
class ChannelSet {
public:
  /// @brief Insert a channel for schema type `T` on the given topic.
  ///
  /// `T` must have a static `schema()` method returning `foxglove::Schema`
  /// (all generated types in `foxglove::schemas` satisfy this).
  /// The message encoding is assumed to be "protobuf".
  ///
  /// @tparam T A Foxglove schema type (e.g. `foxglove::schemas::Vector3`).
  /// @param topic The topic name for this channel.
  template<typename T>
  void insert(const std::string& topic) {
    auto schema = T::schema();
    uint16_t schema_id = add_schema(schema);
    topics.push_back(Topic{topic, "protobuf", schema_id});
  }

  /// @brief The accumulated topics.
  std::vector<Topic> topics;
  /// @brief The accumulated schemas (deduplicated).
  std::vector<Schema> schemas;

private:
  uint16_t next_schema_id_ = 1;

  uint16_t add_schema(const foxglove::Schema& schema) {
    // Check for an existing schema with the same name, encoding, and data.
    for (const auto& existing : schemas) {
      if (existing.name == schema.name && existing.encoding == schema.encoding &&
          existing.data == base64_encode(schema.data, schema.data_len)) {
        return existing.id;
      }
    }
    uint16_t id = next_schema_id_++;
    schemas.push_back(Schema{
      id,
      schema.name,
      schema.encoding,
      base64_encode(schema.data, schema.data_len),
    });
    return id;
  }
};

}  // namespace foxglove::data_provider
