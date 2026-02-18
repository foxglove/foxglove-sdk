use std::sync::OnceLock;

/// Configuration for remote access integration tests.
pub struct Config {
    pub foxglove_api_key: String,
    pub foxglove_api_url: String,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    /// Returns the global config singleton, loading from environment on first access.
    ///
    /// Panics if `FOXGLOVE_API_KEY` is not set.
    pub fn get() -> &'static Config {
        CONFIG.get_or_init(|| {
            let _ = dotenvy::dotenv();
            let foxglove_api_key = std::env::var("FOXGLOVE_API_KEY")
                .expect("FOXGLOVE_API_KEY must be set for auth integration tests");
            let foxglove_api_url = std::env::var("FOXGLOVE_API_URL")
                .unwrap_or_else(|_| "https://api.foxglove.party".to_string());
            Config {
                foxglove_api_key,
                foxglove_api_url,
            }
        })
    }
}
