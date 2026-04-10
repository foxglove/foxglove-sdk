#include "livekit_token.hpp"

#include <jwt-cpp/jwt.h>

#include <chrono>
#include <cstdlib>

namespace foxglove_integration {

std::string livekit_url() {
  const char* env = std::getenv("LIVEKIT_URL");
  if (env != nullptr && env[0] != '\0') {
    return env;
  }
  return "http://localhost:7880";
}

std::string generate_token(const std::string& room_name, const std::string& identity) {
  // LiveKit tokens are JWTs signed with HMAC-SHA256.
  // The payload includes a "video" claim with room grants.
  auto now = std::chrono::system_clock::now();
  auto token = jwt::create()
                 .set_issuer(DEV_API_KEY)
                 .set_subject(identity)
                 .set_not_before(now)
                 .set_issued_at(now)
                 .set_expires_at(now + std::chrono::hours(1))
                 .set_payload_claim(
                   "video",
                   jwt::claim(
                     picojson::value(picojson::object{
                       {"roomJoin", picojson::value(true)},
                       {"room", picojson::value(room_name)},
                     })
                   )
                 )
                 .sign(jwt::algorithm::hs256{DEV_API_SECRET});
  return token;
}

}  // namespace foxglove_integration
