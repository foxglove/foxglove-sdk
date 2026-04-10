#pragma once

#include <string>

namespace foxglove_integration {

constexpr const char* DEV_API_KEY = "devkey";
constexpr const char* DEV_API_SECRET = "secret";

/// Returns the LiveKit dev server URL.
/// Override via the LIVEKIT_URL env var.
std::string livekit_url();

/// Generates a LiveKit access token for the dev server.
/// The token grants room join access to the specified room.
std::string generate_token(const std::string& room_name, const std::string& identity);

}  // namespace foxglove_integration
