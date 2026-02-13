/// @file
/// Example showing how to implement a Foxglove data provider using cpp-httplib.
///
/// This implements the two endpoints required by the HTTP API:
/// - `GET /v1/manifest` - returns a JSON manifest describing the available data
/// - `GET /v1/data` - streams MCAP data
///
/// # Running the example
///
/// See the remote data loader local development guide to test this properly in the Foxglove app.
///
/// You can also test basic functionality with curl:
///
/// To run the example server (from the cpp build directory):
///   ./example_data_provider
///
/// Get a manifest for a specific flight:
///   curl "http://localhost:8080/v1/manifest?flightId=ABC123&startTime=2024-01-01T00:00:00Z&endTime=2024-01-02T00:00:00Z" \
///        -H "Authorization: Bearer test"
///
/// Stream MCAP data:
///   curl "http://localhost:8080/v1/data?flightId=ABC123&startTime=2024-01-01T00:00:00Z&endTime=2024-01-02T00:00:00Z" \
///        -H "Authorization: Bearer test" --output data.mcap
///
/// Verify the MCAP file (requires mcap CLI):
///   mcap info data.mcap

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/schemas.hpp>

#include <httplib.h>
#include <nlohmann/json.hpp>

#include <cerrno>
#include <chrono>
#include <cstdint>
#include <ctime>
#include <iomanip>
#include <iostream>
#include <optional>
#include <sstream>
#include <string>
#include <vector>

using json = nlohmann::json;

// ============================================================================
// Routes
// ============================================================================

// The specific route values are not part of the API; you can change them to whatever you want.
static constexpr const char* MANIFEST_ROUTE = "/v1/manifest";
static constexpr const char* DATA_ROUTE = "/v1/data";
static constexpr int PORT = 8080;

// ============================================================================
// Platform helpers
// ============================================================================

#ifdef _WIN32
static std::time_t make_utc_time(std::tm* tm) {
  return _mkgmtime(tm);
}
static std::tm to_utc_tm(std::time_t time) {
  std::tm tm{};
  gmtime_s(&tm, &time);
  return tm;
}
#else
static std::time_t make_utc_time(std::tm* tm) {
  return timegm(tm);
}
static std::tm to_utc_tm(std::time_t time) {
  std::tm tm{};
  gmtime_r(&time, &tm);
  return tm;
}
#endif

// ============================================================================
// Base64 encoding
// ============================================================================

static const char BASE64_CHARS[] =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

std::string base64_encode(const std::byte* data, size_t len) {
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
    result += BASE64_CHARS[(n >> 18) & 0x3F];
    result += BASE64_CHARS[(n >> 12) & 0x3F];
    result += (i + 1 < len) ? BASE64_CHARS[(n >> 6) & 0x3F] : '=';
    result += (i + 2 < len) ? BASE64_CHARS[n & 0x3F] : '=';
  }
  return result;
}

// ============================================================================
// ISO 8601 timestamp utilities
// ============================================================================

using TimePoint = std::chrono::system_clock::time_point;

/// Parse an ISO 8601 timestamp like "2024-01-01T00:00:00Z".
std::optional<TimePoint> parse_iso8601(const std::string& s) {
  std::tm tm = {};
  std::istringstream ss(s);
  ss >> std::get_time(&tm, "%Y-%m-%dT%H:%M:%S");
  if (ss.fail()) {
    return std::nullopt;
  }
  auto time = make_utc_time(&tm);
  return std::chrono::system_clock::from_time_t(time);
}

/// Format a time_point as ISO 8601.
std::string format_iso8601(TimePoint tp) {
  auto tt = std::chrono::system_clock::to_time_t(tp);
  std::tm tm = to_utc_tm(tt);
  std::ostringstream ss;
  ss << std::put_time(&tm, "%Y-%m-%dT%H:%M:%SZ");
  return ss.str();
}

/// Convert time_point to nanoseconds since epoch.
uint64_t to_nanos(TimePoint tp) {
  auto duration = tp.time_since_epoch();
  return static_cast<uint64_t>(
    std::chrono::duration_cast<std::chrono::nanoseconds>(duration).count()
  );
}

/// Round a time_point up to the next second boundary.
TimePoint round_up_to_second(TimePoint tp) {
  auto secs = std::chrono::duration_cast<std::chrono::seconds>(tp.time_since_epoch());
  auto rounded = TimePoint(secs);
  if (rounded < tp) {
    rounded += std::chrono::seconds(1);
  }
  return rounded;
}

// ============================================================================
// URL encoding
// ============================================================================

std::string url_encode(const std::string& s) {
  std::ostringstream escaped;
  escaped.fill('0');
  escaped << std::hex;
  for (char c : s) {
    if (std::isalnum(static_cast<unsigned char>(c)) || c == '-' || c == '_' || c == '.' ||
        c == '~') {
      escaped << c;
    } else {
      escaped << std::uppercase << '%' << std::setw(2)
              << static_cast<int>(static_cast<unsigned char>(c)) << std::nouppercase;
    }
  }
  return escaped.str();
}

// ============================================================================
// Manifest types (serialized as JSON matching the Foxglove data provider API)
// ============================================================================

/// A topic in a streamed source.
struct ManifestTopic {
  std::string name;
  std::string message_encoding;
  uint16_t schema_id;
};

void to_json(json& j, const ManifestTopic& t) {
  j = json{
    {"name", t.name},
    {"messageEncoding", t.message_encoding},
    {"schemaId", t.schema_id},
  };
}

/// A schema in a streamed source.
struct ManifestSchema {
  uint16_t id;
  std::string name;
  std::string encoding;
  std::string data;  // base64-encoded
};

void to_json(json& j, const ManifestSchema& s) {
  j = json{
    {"id", s.id},
    {"name", s.name},
    {"encoding", s.encoding},
    {"data", s.data},
  };
}

/// A streamed data source.
struct StreamedSource {
  std::string url;
  std::string id;
  std::vector<ManifestTopic> topics;
  std::vector<ManifestSchema> schemas;
  std::string start_time;  // ISO 8601
  std::string end_time;    // ISO 8601
};

void to_json(json& j, const StreamedSource& s) {
  j = json{
    {"url", s.url},
    {"id", s.id},
    {"topics", s.topics},
    {"schemas", s.schemas},
    {"startTime", s.start_time},
    {"endTime", s.end_time},
  };
}

/// The manifest returned by the manifest endpoint.
struct Manifest {
  std::string name;
  std::vector<StreamedSource> sources;
};

void to_json(json& j, const Manifest& m) {
  j = json{
    {"name", m.name},
    {"sources", m.sources},
  };
}

// ============================================================================
// Flight parameters (parsed from query parameters)
// ============================================================================

struct FlightParams {
  std::string flight_id;
  TimePoint start_time;
  TimePoint end_time;

  /// Build a query string for these parameters.
  std::string to_query_string() const {
    return "flightId=" + url_encode(flight_id) +
      "&startTime=" + url_encode(format_iso8601(start_time)) +
      "&endTime=" + url_encode(format_iso8601(end_time));
  }
};

/// Parse flight parameters from request query string.
/// Returns nullopt if required parameters are missing or invalid.
std::optional<FlightParams> parse_flight_params(const httplib::Request& req) {
  if (!req.has_param("flightId") || !req.has_param("startTime") || !req.has_param("endTime")) {
    return std::nullopt;
  }
  FlightParams params;
  params.flight_id = req.get_param_value("flightId");
  auto start = parse_iso8601(req.get_param_value("startTime"));
  auto end = parse_iso8601(req.get_param_value("endTime"));
  if (!start || !end) {
    return std::nullopt;
  }
  params.start_time = *start;
  params.end_time = *end;
  return params;
}

// ============================================================================
// Auth
// ============================================================================

/// Validate the bearer token from the Authorization header.
///
/// Replace this with real token validation (e.g. JWT verification).
bool check_auth(const httplib::Request& req) {
  auto it = req.headers.find("Authorization");
  if (it == req.headers.end()) {
    return false;
  }
  const auto& value = it->second;
  // Accept any non-empty bearer token.
  if (value.size() <= 7) {
    return false;
  }
  return value.substr(0, 7) == "Bearer ";
}

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `GET /v1/manifest`.
///
/// Builds a manifest describing the channels and schemas available for the requested flight.
void manifest_handler(const httplib::Request& req, httplib::Response& res) {
  if (!check_auth(req)) {
    res.status = 401;
    res.set_content("Unauthorized", "text/plain");
    return;
  }

  auto params = parse_flight_params(req);
  if (!params) {
    res.status = 400;
    res.set_content("Missing or invalid query parameters", "text/plain");
    return;
  }

  // Declare a single channel of Foxglove `Vector3` messages on topic "/demo".
  auto schema = foxglove::schemas::Vector3::schema();
  auto query = params->to_query_string();

  StreamedSource source;
  // We're providing the data from this service in this example, but in principle this could
  // be any URL.
  source.url = std::string(DATA_ROUTE) + "?" + query;
  // `id` must be unique to this data source. Otherwise, incorrect data may be served from cache.
  //
  // Here we reuse the query string to make sure we don't forget any parameters. We also
  // include a version number we increment whenever we change the data handler.
  source.id = "flight-v1-" + query;
  source.topics = {ManifestTopic{"/demo", "protobuf", 1}};
  source.schemas = {ManifestSchema{
    1,
    schema.name,
    schema.encoding,
    base64_encode(schema.data, schema.data_len),
  }};
  source.start_time = format_iso8601(params->start_time);
  source.end_time = format_iso8601(params->end_time);

  Manifest manifest;
  manifest.name = "Flight " + params->flight_id;
  manifest.sources = {std::move(source)};

  json j = manifest;
  res.set_content(j.dump(), "application/json");
}

/// Handler for `GET /v1/data`.
///
/// Streams MCAP data for the requested flight. The response body is a stream of MCAP bytes.
void data_handler(const httplib::Request& req, httplib::Response& res) {
  if (!check_auth(req)) {
    res.status = 401;
    res.set_content("Unauthorized", "text/plain");
    return;
  }

  auto params = parse_flight_params(req);
  if (!params) {
    res.status = 400;
    res.set_content("Missing or invalid query parameters", "text/plain");
    return;
  }

  // Capture parameters for the content provider lambda (which may outlive this handler).
  auto flight_params = std::move(*params);

  res.set_chunked_content_provider(
    "application/octet-stream",
    [flight_params = std::move(flight_params)](size_t /*offset*/, httplib::DataSink& sink) {
      // Create a dedicated context for this request's MCAP output.
      auto context = foxglove::Context::create();

      // Buffer that accumulates MCAP bytes written by the CustomWriter.
      std::vector<uint8_t> buffer;
      // Track current write position for seek queries.
      uint64_t write_position = 0;

      foxglove::CustomWriter custom_writer;
      custom_writer.write = [&buffer, &write_position](
                              const uint8_t* data, size_t len, int* /*error*/
                            ) -> size_t {
        buffer.insert(buffer.end(), data, data + len);
        write_position += len;
        return len;
      };
      custom_writer.flush = []() -> int {
        return 0;
      };
      // Support position queries (SEEK_CUR with offset 0) but reject actual seeking.
      // The MCAP writer may query the current position even with disable_seeking = true.
      custom_writer.seek = [&write_position](int64_t pos, int whence, uint64_t* new_pos) -> int {
        // SEEK_CUR (1) with offset 0: return current position.
        if (whence == 1 && pos == 0) {
          if (new_pos != nullptr) {
            *new_pos = write_position;
          }
          return 0;
        }
        // SEEK_SET (0) to the current position: no-op.
        if (whence == 0 && static_cast<uint64_t>(pos) == write_position) {
          if (new_pos != nullptr) {
            *new_pos = write_position;
          }
          return 0;
        }
        // Actual seeking is not supported for streaming.
        return EIO;
      };

      foxglove::McapWriterOptions options;
      options.context = context;
      options.custom_writer = custom_writer;
      options.disable_seeking = true;
      options.compression = foxglove::McapCompression::None;
      // Use a smaller chunk size for more incremental streaming.
      options.chunk_size = 64 * 1024;

      auto writer_result = foxglove::McapWriter::create(options);
      if (!writer_result.has_value()) {
        std::cerr << "[data_provider] failed to create MCAP writer: "
                  << foxglove::strerror(writer_result.error()) << "\n";
        sink.done();
        return false;
      }
      auto writer = std::move(writer_result.value());

      // Declare channels.
      auto channel_result = foxglove::schemas::Vector3Channel::create("/demo", context);
      if (!channel_result.has_value()) {
        std::cerr << "[data_provider] failed to create channel: "
                  << foxglove::strerror(channel_result.error()) << "\n";
        sink.done();
        return false;
      }
      auto channel = std::move(channel_result.value());

      // In this example, we query a simulated dataset, but in a real implementation you would
      // probably query a database or other storage.
      //
      // This simulated dataset consists of messages emitted every second from the Unix epoch.
      std::cerr << "[data_provider] streaming data for flight " << flight_params.flight_id << "\n";

      // Clamp start time to epoch (ignore negative start times).
      auto start = flight_params.start_time;
      auto epoch = TimePoint{};
      if (start < epoch) {
        start = epoch;
      }

      // Compute timestamp of first message by rounding the start time up to the second.
      auto ts = round_up_to_second(start);

      while (ts <= flight_params.end_time) {
        // Messages in the output MUST appear in ascending timestamp order. Otherwise, playback
        // will be incorrect.
        auto secs_since_epoch = std::chrono::duration_cast<std::chrono::seconds>(
          ts.time_since_epoch()
        );

        foxglove::schemas::Vector3 msg;
        msg.x = static_cast<double>(secs_since_epoch.count());
        msg.y = 0.0;
        msg.z = 0.0;

        channel.log(msg, to_nanos(ts));

        // Periodically flush buffered data to the response stream. This serves two purposes:
        // the client receives data incrementally instead of all at once, and memory usage stays
        // bounded instead of growing with the entire recording.
        constexpr size_t FLUSH_THRESHOLD = 1024 * 1024;
        if (buffer.size() >= FLUSH_THRESHOLD) {
          if (!sink.write(reinterpret_cast<const char*>(buffer.data()), buffer.size())) {
            std::cerr << "[data_provider] client disconnected\n";
            return false;
          }
          buffer.clear();
        }

        ts += std::chrono::seconds(1);
      }

      // Finalize the MCAP and ensure it is sent to the client.
      auto err = writer.close();
      if (err != foxglove::FoxgloveError::Ok) {
        std::cerr << "[data_provider] error closing MCAP writer: " << foxglove::strerror(err)
                  << "\n";
      }

      // Flush any remaining data.
      if (!buffer.empty()) {
        sink.write(reinterpret_cast<const char*>(buffer.data()), buffer.size());
      }

      sink.done();
      return false;
    }
  );
}

// ============================================================================
// Main
// ============================================================================

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  httplib::Server svr;

  svr.Get(MANIFEST_ROUTE, manifest_handler);
  svr.Get(DATA_ROUTE, data_handler);

  std::cerr << "[data_provider] starting server on 0.0.0.0:" << PORT << "\n";
  svr.listen("0.0.0.0", PORT);

  return 0;
}
