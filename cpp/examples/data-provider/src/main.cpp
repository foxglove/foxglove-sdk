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
///   curl "http://localhost:8081/v1/manifest?flightId=ABC123&startTime=2024-01-01T00:00:00Z\
///     &endTime=2024-01-02T00:00:00Z"
/// @endcode
///
/// Stream MCAP data:
/// @code{.sh}
///   curl --output data.mcap "http://localhost:8081/v1/data?flightId=ABC123\
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

#include <algorithm>
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

// ============================================================================
// Routes
// ============================================================================

// The specific route values are not part of the API; you can change them to whatever you want.
static constexpr const char* MANIFEST_ROUTE = "/v1/manifest";
static constexpr const char* DATA_ROUTE = "/v1/data";
static constexpr int PORT = 8081;

// ============================================================================
// Flight parameters (parsed from query parameters)
// ============================================================================

struct FlightParams {
  std::string flight_id;
  system_clock::time_point start_time;
  system_clock::time_point end_time;

  /// Build a query string for these parameters.
  std::string to_query_string() const {
    std::string q;
    q += "flightId=";
    q += httplib::encode_uri_component(flight_id);
    q += "&startTime=";
    q += httplib::encode_uri_component(format_iso8601(start_time));
    q += "&endTime=";
    q += httplib::encode_uri_component(format_iso8601(end_time));
    return q;
  }
};

/// Parse flight parameters from the request query string.
/// Returns the parsed parameters, or nullopt after setting a 400 response if invalid.
std::optional<FlightParams> require_flight_params(
  const httplib::Request& req, httplib::Response& res
) {
  if (!req.has_param("flightId") || !req.has_param("startTime") || !req.has_param("endTime")) {
    res.status = 400;
    res.set_content("Missing required query parameters", "text/plain");
    return std::nullopt;
  }
  auto start = parse_iso8601(req.get_param_value("startTime"));
  auto end = parse_iso8601(req.get_param_value("endTime"));
  if (!start || !end) {
    res.status = 400;
    res.set_content("Invalid timestamp format", "text/plain");
    return std::nullopt;
  }
  FlightParams params;
  params.flight_id = req.get_param_value("flightId");
  params.start_time = *start;
  params.end_time = *end;
  return params;
}

// ============================================================================
// Auth
// ============================================================================

/// Check the bearer token to see if the user is authorized to access the flight.
/// Returns true if the request is authorized; sets a 401 response and returns false otherwise.
bool require_auth(
  const httplib::Request& /*req*/, const FlightParams& /*params*/, httplib::Response& /*res*/
) {
  // EXAMPLE ONLY: REPLACE THIS WITH A REAL AUTH CHECK.
  return true;
}

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `GET /v1/manifest`.
///
/// Builds a manifest describing the channels and schemas available for the requested flight.
///
/// The user **MUST** be authorized to read all sources returned in the manifest. Do not rely
/// on authorization checks on individual sources, because they may not be called for cached data.
void manifest_handler(const httplib::Request& req, httplib::Response& res) {
  auto params = require_flight_params(req, res);
  if (!params) {
    return;
  }

  if (!require_auth(req, *params, res)) {
    return;
  }

  // Declare a single channel of Foxglove `Vector3` messages on topic "/demo".
  dp::ChannelSet channels;
  channels.insert<foxglove::schemas::Vector3>("/demo");

  auto query = params->to_query_string();

  dp::StreamedSource source;
  // We're providing the data from this service in this example, but in principle this could
  // be any URL.
  source.url = DATA_ROUTE + std::string("?") + query;
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

  res.set_content(dp::to_json_string(manifest), "application/json");
}

/// Handler for `GET /v1/data`.
///
/// Streams MCAP data for the requested flight. The response body is a stream of MCAP bytes.
void data_handler(const httplib::Request& req, httplib::Response& res) {
  auto params = require_flight_params(req, res);
  if (!params) {
    return;
  }

  if (!require_auth(req, *params, res)) {
    return;
  }

  res.set_chunked_content_provider(
    "application/octet-stream",
    [params = std::move(*params)](size_t /*offset*/, httplib::DataSink& sink) {
      // Create a dedicated context for this request's MCAP output.
      auto context = foxglove::Context::create();

      bool write_ok = true;

      // The CustomWriter sends MCAP data directly to the HTTP socket. The MCAP writer
      // buffers internally (up to chunk_size bytes) before calling write, so each call
      // here corresponds to one MCAP chunk being flushed.
      uint64_t position = 0;
      foxglove::CustomWriter custom_writer;
      custom_writer.write =
        [&position, &sink, &write_ok](const uint8_t* data, size_t len, int* error) -> size_t {
        if (sink.write(reinterpret_cast<const char*>(data), len)) {
          position += len;
          return len;
        }
        *error = EIO;
        write_ok = false;
        return 0;
      };
      custom_writer.flush = []() -> int {
        // httplib manages flushing itself, so we don't do anything here.
        return 0;
      };
      custom_writer.seek = foxglove::no_seek_fn(&position);

      foxglove::McapWriterOptions options;
      options.context = context;
      options.custom_writer = custom_writer;
      options.disable_seeking = true;
      options.chunk_size = 64 * 1024;

      auto writer_result = foxglove::McapWriter::create(options);
      if (!writer_result.has_value()) {
        std::cerr << "[data_provider] failed to create MCAP writer: "
                  << foxglove::strerror(writer_result.error()) << "\n";
        sink.done();
        return false;
      }
      auto writer = std::move(writer_result.value());

      auto channel_result = foxglove::schemas::Vector3Channel::create("/demo", context);
      if (!channel_result.has_value()) {
        std::cerr << "[data_provider] failed to create channel: "
                  << foxglove::strerror(channel_result.error()) << "\n";
        sink.done();
        return false;
      }
      auto channel = std::move(channel_result.value());

      // In this example, we query a simulated dataset, but in a real implementation you would
      // probably query a database or other storage. Because the CustomWriter sends data directly
      // to the client, you can iterate over a database cursor here and the client will receive
      // data incrementally as MCAP chunks are flushed.
      //
      // This simulated dataset consists of messages emitted every second from the Unix epoch.
      std::cerr << "[data_provider] streaming data for flight " << params.flight_id << "\n";

      auto start = std::max(params.start_time, system_clock::time_point{});
      auto ts = date::ceil<std::chrono::seconds>(start);

      while (write_ok && ts <= params.end_time) {
        // Messages in the output MUST appear in ascending timestamp order. Otherwise, playback
        // will be incorrect.
        foxglove::schemas::Vector3 msg;
        msg.x = static_cast<double>(
          std::chrono::duration_cast<std::chrono::seconds>(ts.time_since_epoch()).count()
        );
        msg.y = 0.0;
        msg.z = 0.0;

        // Log with an explicit nanosecond timestamp. This assumes system_clock uses the
        // Unix epoch, which is guaranteed by C++20 but not C++17 (true in practice on all
        // major implementations).
        channel.log(
          msg,
          static_cast<uint64_t>(date::floor<std::chrono::nanoseconds>(ts).time_since_epoch().count()
          )
        );

        ts += std::chrono::seconds(1);
      }

      if (!write_ok) {
        std::cerr << "[data_provider] client disconnected\n";
        return false;
      }

      // Finalize the MCAP file (writes header/footer via the CustomWriter to the socket).
      auto err = writer.close();
      if (err != foxglove::FoxgloveError::Ok) {
        std::cerr << "[data_provider] error closing MCAP writer: " << foxglove::strerror(err)
                  << "\n";
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
