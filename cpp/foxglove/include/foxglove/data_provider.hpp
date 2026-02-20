#pragma once

/// @file
/// Types and utilities for building remote data loader manifests.
///
/// Use @ref foxglove::data_provider::ChannelSet to declare channels, then construct a
/// @ref foxglove::data_provider::StreamedSource with the resulting topics and schemas.
///
/// @note This header requires [nlohmann/json](https://github.com/nlohmann/json) and
/// [tobiaslocker/base64](https://github.com/tobiaslocker/base64) to be available on the include
/// path.
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
/// std::string json_str = dp::to_json_string(manifest);
/// @endcode

#include <foxglove/schema.hpp>

#include <nlohmann/json.hpp>

#include <base64.hpp>
#include <cstdint>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

/// The foxglove namespace.
namespace foxglove::data_provider {

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
  /// @brief Unique schema ID within this source. Must be nonzero.
  uint16_t id;
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
// JSON serialization
// ============================================================================

/// @brief Serialize a Manifest to a JSON string.
///
/// The output conforms to the Foxglove data provider manifest JSON schema.
inline std::string to_json_string(const Manifest& m) {
  // Use nlohmann/json internally without exposing it in the public API.
  using json = nlohmann::json;

  auto topic_to_json = [](const Topic& t) -> json {
    json j{
      {"name", t.name},
      {"messageEncoding", t.message_encoding},
    };
    if (t.schema_id.has_value()) {
      j["schemaId"] = *t.schema_id;
    }
    return j;
  };

  auto schema_to_json = [](const Schema& s) -> json {
    return json{
      {"id", s.id},
      {"name", s.name},
      {"encoding", s.encoding},
      {"data", s.data},
    };
  };

  auto source_to_json = [&](const StreamedSource& s) -> json {
    json topics = json::array();
    for (const auto& t : s.topics) {
      topics.push_back(topic_to_json(t));
    }
    json schemas = json::array();
    for (const auto& sc : s.schemas) {
      schemas.push_back(schema_to_json(sc));
    }
    json j{
      {"url", s.url},
      {"topics", topics},
      {"schemas", schemas},
      {"startTime", s.start_time},
      {"endTime", s.end_time},
    };
    if (s.id.has_value()) {
      j["id"] = *s.id;
    }
    return j;
  };

  json sources = json::array();
  for (const auto& s : m.sources) {
    sources.push_back(source_to_json(s));
  }
  json j{
    {"sources", sources},
  };
  if (m.name.has_value()) {
    j["name"] = *m.name;
  }
  return j.dump();
}

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
  /// @throws std::overflow_error if more than 65535 distinct schemas are added.
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
  // Next schema ID to assign. 0 means we have exhausted all IDs.
  // Schema ID 0 is reserved by MCAP, so valid IDs are 1..65535.
  uint16_t next_schema_id_ = 1;

  static std::string encode_schema_data(const foxglove::Schema& schema) {
    std::string_view sv(reinterpret_cast<const char*>(schema.data), schema.data_len);
    return base64::to_base64(sv);
  }

  uint16_t add_schema(const foxglove::Schema& schema) {
    auto encoded_data = encode_schema_data(schema);

    // Deduplicate: return existing ID if an identical schema was already added.
    for (const auto& existing : schemas) {
      if (existing.name == schema.name && existing.encoding == schema.encoding &&
          existing.data == encoded_data) {
        return existing.id;
      }
    }

    if (next_schema_id_ == 0) {
      throw std::overflow_error("ChannelSet: cannot add more than 65535 schemas");
    }
    uint16_t id = next_schema_id_++;  // wraps to 0 after 65535

    schemas.push_back(Schema{
      id,
      schema.name,
      schema.encoding,
      std::move(encoded_data),
    });
    return id;
  }
};

}  // namespace foxglove::data_provider
