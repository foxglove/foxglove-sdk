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

#include <algorithm>
#include <cerrno>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <httplib.h>
#include <iostream>
#include <memory>
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
/// Returns true if the request is authorized; sets a 401 response and returns false otherwise.
///
/// Replace this with real token validation (e.g. JWT verification).
bool require_auth(const httplib::Request& req, httplib::Response& res) {
  auto it = req.headers.find("Authorization");
  if (it != req.headers.end()) {
    const auto& value = it->second;
    // Accept any non-empty bearer token.
    if (value.size() > 7 && value.compare(0, 7, "Bearer ") == 0) {
      return true;
    }
  }
  res.status = 401;
  res.set_content("Unauthorized", "text/plain");
  return false;
}

// ============================================================================
// MCAP streaming state
// ============================================================================

/// Holds the MCAP writer infrastructure and an intermediate buffer for streaming MCAP data
/// to an HTTP response. This is allocated once per request and shared into the chunked content
/// provider lambda via shared_ptr.
struct McapStreamState {
  std::vector<uint8_t> buffer;
  uint64_t write_position = 0;

  /// Create a CustomWriter that appends to this state's buffer.
  foxglove::CustomWriter make_custom_writer() {
    foxglove::CustomWriter cw;
    cw.write = [this](const uint8_t* data, size_t len, int* /*error*/) -> size_t {
      buffer.insert(buffer.end(), data, data + len);
      write_position += len;
      return len;
    };
    cw.flush = []() -> int {
      return 0;
    };
    // Support position queries but reject actual seeking. The MCAP writer may query
    // the current position even with disable_seeking = true.
    cw.seek = [this](int64_t pos, int whence, uint64_t* new_pos) -> int {
      if (whence == SEEK_CUR && pos == 0) {
        if (new_pos != nullptr) {
          *new_pos = write_position;
        }
        return 0;
      }
      if (whence == SEEK_SET && static_cast<uint64_t>(pos) == write_position) {
        if (new_pos != nullptr) {
          *new_pos = write_position;
        }
        return 0;
      }
      return EIO;
    };
    return cw;
  }

  /// Flush any buffered MCAP bytes to the HTTP response. Returns false if the client
  /// disconnected.
  bool flush_to(httplib::DataSink& sink) {
    if (buffer.empty()) {
      return true;
    }
    bool ok = sink.write(reinterpret_cast<const char*>(buffer.data()), buffer.size());
    buffer.clear();
    return ok;
  }
};

// ============================================================================
// Handlers
// ============================================================================

/// Handler for `GET /v1/manifest`.
///
/// Builds a manifest describing the channels and schemas available for the requested flight.
void manifest_handler(const httplib::Request& req, httplib::Response& res) {
  if (!require_auth(req, res)) {
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

  nlohmann::json j = manifest;
  res.set_content(j.dump(), "application/json");
}

/// Handler for `GET /v1/data`.
///
/// Streams MCAP data for the requested flight. The response body is a stream of MCAP bytes.
/// The chunked content provider lambda is called repeatedly by httplib; each invocation
/// produces a batch of messages and flushes them to the client, keeping memory usage bounded.
void data_handler(const httplib::Request& req, httplib::Response& res) {
  if (!require_auth(req, res)) {
    return;
  }
  auto params = parse_flight_params(req);
  if (!params) {
    res.status = 400;
    res.set_content("Missing or invalid query parameters", "text/plain");
    return;
  }

  // Set up MCAP streaming state once, shared into the content provider lambda.
  auto state = std::make_shared<McapStreamState>();

  auto context = foxglove::Context::create();

  foxglove::McapWriterOptions options;
  options.context = context;
  options.custom_writer = state->make_custom_writer();
  options.disable_seeking = true;
  options.compression = foxglove::McapCompression::None;
  options.chunk_size = 64 * 1024;

  auto writer_result = foxglove::McapWriter::create(options);
  if (!writer_result.has_value()) {
    std::cerr << "[data_provider] failed to create MCAP writer: "
              << foxglove::strerror(writer_result.error()) << "\n";
    res.status = 500;
    res.set_content("Internal error", "text/plain");
    return;
  }

  auto channel_result = foxglove::schemas::Vector3Channel::create("/demo", context);
  if (!channel_result.has_value()) {
    std::cerr << "[data_provider] failed to create channel: "
              << foxglove::strerror(channel_result.error()) << "\n";
    res.status = 500;
    res.set_content("Internal error", "text/plain");
    return;
  }

  // In this example, we query a simulated dataset, but in a real implementation you would
  // probably query a database or other storage.
  //
  // This simulated dataset consists of messages emitted every second from the Unix epoch.
  std::cerr << "[data_provider] streaming data for flight " << params->flight_id << "\n";

  auto start = std::max(params->start_time, system_clock::time_point{});
  auto first_ts = date::ceil<std::chrono::seconds>(start);

  // The content provider lambda is called repeatedly by httplib. Each call produces a batch
  // of messages and streams them to the client.
  //
  // McapWriter and Vector3Channel are move-only, so we wrap them in shared_ptr to satisfy
  // std::function's copy requirement.
  //
  // To adapt this for a real data source, replace the timestamp loop with e.g. a database
  // cursor, producing a batch of rows per invocation.
  constexpr size_t BATCH_SIZE = 1024;
  auto writer = std::make_shared<foxglove::McapWriter>(std::move(writer_result.value()));
  auto channel =
    std::make_shared<foxglove::schemas::Vector3Channel>(std::move(channel_result.value()));

  res.set_chunked_content_provider(
    "application/octet-stream",
    [state,
     context = std::move(context),
     writer,
     channel,
     end_time = params->end_time,
     ts = first_ts,
     batch_size = BATCH_SIZE](size_t /*offset*/, httplib::DataSink& sink) mutable -> bool {
      // Generate a batch of messages.
      for (size_t i = 0; i < batch_size && ts <= end_time; ++i, ts += std::chrono::seconds(1)) {
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
        channel->log(
          msg,
          static_cast<uint64_t>(date::floor<std::chrono::nanoseconds>(ts).time_since_epoch().count()
          )
        );
      }

      // Flush buffered MCAP data to the HTTP response.
      if (!state->flush_to(sink)) {
        std::cerr << "[data_provider] client disconnected\n";
        return false;
      }

      // If we've sent all messages, finalize the MCAP and close the stream.
      if (ts > end_time) {
        auto err = writer->close();
        if (err != foxglove::FoxgloveError::Ok) {
          std::cerr << "[data_provider] error closing MCAP writer: " << foxglove::strerror(err)
                    << "\n";
        }
        state->flush_to(sink);
        sink.done();
        return false;
      }

      return true;
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
