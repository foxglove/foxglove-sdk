use anyhow::Result;
use livekit_api::access_token::{AccessToken, VideoGrants};

/// Default LiveKit dev server credentials.
const DEV_API_KEY: &str = "devkey";
const DEV_API_SECRET: &str = "secret";

/// Returns the LiveKit dev server URL. Override via the `LIVEKIT_URL` env var
/// when running inside a Docker container (e.g., on the perlink network).
pub fn livekit_url() -> String {
    std::env::var("LIVEKIT_URL").unwrap_or_else(|_| "http://localhost:7880".to_string())
}

/// Generates a LiveKit access token for the dev server.
///
/// The token grants room join access to the specified room.
pub fn generate_token(room_name: &str, identity: &str) -> Result<String> {
    let grants = VideoGrants {
        room_join: true,
        room: room_name.to_string(),
        ..Default::default()
    };
    let token = AccessToken::with_api_key(DEV_API_KEY, DEV_API_SECRET)
        .with_identity(identity)
        .with_grants(grants)
        .to_jwt()?;
    Ok(token)
}
