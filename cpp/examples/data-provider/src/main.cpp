/// @file
/// Example showing how to implement a Foxglove data provider using cpp-httplib.
///
/// This implements the two endpoints required by the HTTP API:
/// - `GET /v1/manifest` - returns a JSON manifest describing the available data
/// - `GET /v1/data` - streams MCAP data
///
/// # Running the example
///
/// See the remote data loader local development guide to test this properly
/// in the Foxglove app.
///
/// You can also test basic functionality with curl:
///
/// To run the example server (from the cpp build directory):
/// @code{.sh}
///   ./example_data_provider
/// @endcode
///
/// Get a manifest for a specific flight:
/// @code{.sh}
///   curl -H "Authorization: Bearer test" \
///     "http://localhost:8080/v1/manifest?flightId=ABC123\
///     &startTime=2024-01-01T00:00:00Z&endTime=2024-01-02T00:00:00Z"
/// @endcode
///
/// Stream MCAP data:
/// @code{.sh}
///   curl -H "Authorization: Bearer test" --output data.mcap \
///     "http://localhost:8080/v1/data?flightId=ABC123\
///     &startTime=2024-01-01T00:00:00Z&endTime=2024-01-02T00:00:00Z"
/// @endcode
///
/// Verify the MCAP file (requires mcap CLI):
/// @code{.sh}
///   mcap info data.mcap
/// @endcode

#include <foxglove/context.hpp>
#include <foxglove/data_provider.hpp>
#include <foxglove/error.hpp>
#include <foxglove/mcap.hpp>
#include <foxglove/schemas.hpp>

#include <date/date.h>

#include <cerrno>
#include <chrono>
#include <cstdint>
#include <httplib.h>
#include <iostream>
#include <optional>
#include <sstream>
#include <string>

namespace dp = foxglove::data_provider;
using std::chrono::system_clock;

// ============================================================================
// Timestamp helpers (using Howard Hinnant's date library)
// ============================================================================

/// Parse an ISO 8601 timestamp like "2024-01-01T00:00:00Z".
std::optional<system_clock::time_point> parse_iso8601(const std::string& s) {
  std::istringstream ss(s);
  system_clock::time_point tp;
  ss >> date::parse("%FT%TZ", tp);
  if (ss.fail()) {
    return std::nullopt;
  }
  return tp;
}

/// Format a time_point as ISO 8601.
std::string format_iso8601(system_clock::time_point tp) {
  return date::format("%FT%TZ", date::floor<std::chrono::seconds>(tp));
}

/// Convert a system_clock time_point to nanoseconds since epoch.
///
/// This assumes system_clock's epoch is the Unix epoch, which is guaranteed by C++20
/// but not by C++17. In practice all major implementations use the Unix epoch.
uint64_t to_nanos(system_clock::time_point tp) {
  return static_cast<uint64_t>(
    std::chrono::duration_cast<std::chrono::nanoseconds>(tp.time_since_epoch()).count()
  );
}

// ============================================================================
// Routes
// ============================================================================

// The specific route values are not part of the API; you can change them to whatever you want.
static constexpr const char* MANIFEST_ROUTE = "/v1/manifest";
static constexpr const char* DATA_ROUTE = "/v1/data";
static constexpr int PORT = 8080;

// ============================================================================
// Flight parameters (parsed from query parameters)
// ============================================================================

struct FlightParams {
  std::string flight_id;
  system_clock::time_point start_time;
  system_clock::time_point end_time;

  /// Build a query string for these parameters.
  std::string to_query_string() const {
    return "flightId=" + httplib::encode_uri_component(flight_id) +
           "&startTime=" + httplib::encode_uri_component(format_iso8601(start_time)) +
           "&endTime=" + httplib::encode_uri_component(format_iso8601(end_time));
  }
};

/// Parse flight parameters from request query string.
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
  return value.size() > 7 && value.substr(0, 7) == "Bearer ";
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
  dp::ChannelSet channels;
  channels.insert<foxglove::schemas::Vector3>("/demo");

  auto query = params->to_query_string();

  dp::StreamedSource source;
  // We're providing the data from this service in this example, but in principle this could
  // be any URL.
  source.url = std::string(DATA_ROUTE) + "?" + query;
  // `id` must be unique to this data source. Otherwise, incorrect data may be served from cache.
  //
  // Here we reuse the query string to make sure we don't forget any parameters. We also
  // include a version number we increment whenever we change the data handler.
  source.id = "flight-v1-" + query;
  source.topics = std::move(channels.topics);
  source.schemas = std::move(channels.schemas);
  source.start_time = format_iso8601(params->start_time);
  source.end_time = format_iso8601(params->end_time);

  dp::Manifest manifest;
  manifest.name = "Flight " + params->flight_id;
  manifest.sources = {std::move(source)};

  nlohmann::json j = manifest;
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
        if (whence == 1 && pos == 0) {
          if (new_pos != nullptr) {
            *new_pos = write_position;
          }
          return 0;
        }
        if (whence == 0 && static_cast<uint64_t>(pos) == write_position) {
          if (new_pos != nullptr) {
            *new_pos = write_position;
          }
          return 0;
        }
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
      auto epoch = system_clock::time_point{};
      if (start < epoch) {
        start = epoch;
      }

      // Compute timestamp of first message by rounding the start time up to the second.
      auto ts = date::ceil<std::chrono::seconds>(start);

      while (ts <= flight_params.end_time) {
        // Messages in the output MUST appear in ascending timestamp order. Otherwise, playback
        // will be incorrect.
        auto secs_since_epoch =
          std::chrono::duration_cast<std::chrono::seconds>(ts.time_since_epoch());

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
