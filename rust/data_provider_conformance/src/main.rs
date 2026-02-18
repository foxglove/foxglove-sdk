//! CLI runner for the data provider conformance test suite.
//!
//! Connects to (or spawns) a data provider server and runs all conformance checks.
//!
//! # Environment variables
//!
//! - `DATA_PROVIDER_URL` (required) — full manifest URL including query parameters,
//!   e.g. `http://127.0.0.1:8081/v1/manifest?flightId=TEST&startTime=...&endTime=...`
//! - `DATA_PROVIDER_CMD` — path to a server binary to spawn; if unset, the server must already be
//!   running
//! - `DATA_PROVIDER_ADDR` — socket address to wait for when spawning a server (required if
//!   `DATA_PROVIDER_CMD` is set), e.g. `127.0.0.1:8081`
//! - `DATA_PROVIDER_BEARER_TOKEN` — bearer token for authentication (default: `test-token`)
//!
//! # Examples
//!
//! Test a server that is already running:
//!
//! ```sh
//! DATA_PROVIDER_URL="http://127.0.0.1:8081/v1/manifest?flightId=TEST&startTime=2024-01-01T00:00:00Z&endTime=2024-01-01T00:00:05Z" \
//!   cargo run -p data_provider_conformance
//! ```
//!
//! Spawn a server binary and test it:
//!
//! ```sh
//! DATA_PROVIDER_CMD=cpp/build/example_data_provider \
//! DATA_PROVIDER_ADDR=127.0.0.1:8081 \
//! DATA_PROVIDER_URL="http://127.0.0.1:8081/v1/manifest?flightId=TEST&startTime=2024-01-01T00:00:00Z&endTime=2024-01-01T00:00:05Z" \
//!   cargo run -p data_provider_conformance
//! ```

use std::process::ExitCode;

use data_provider_conformance::DataProviderTestConfig;

fn required_env(key: &str) -> String {
    std::env::var_os(key)
        .unwrap_or_else(|| {
            eprintln!("error: {key} environment variable must be set");
            std::process::exit(2);
        })
        .into_string()
        .unwrap_or_else(|_| panic!("{key} must be valid Unicode"))
}

fn main() -> ExitCode {
    let manifest_url = required_env("DATA_PROVIDER_URL")
        .parse()
        .unwrap_or_else(|e| {
            eprintln!("error: DATA_PROVIDER_URL is not a valid URL: {e}");
            std::process::exit(2);
        });

    let bearer_token = std::env::var_os("DATA_PROVIDER_BEARER_TOKEN")
        .map(|v| {
            v.into_string()
                .expect("DATA_PROVIDER_BEARER_TOKEN must be valid Unicode")
        })
        .unwrap_or_else(|| "test-token".into());

    // If DATA_PROVIDER_CMD is set, spawn the server binary and keep it alive for the test run.
    let _guard = std::env::var_os("DATA_PROVIDER_CMD").map(|cmd| {
        let addr = required_env("DATA_PROVIDER_ADDR");
        data_provider_conformance::spawn_server(cmd, &addr)
    });

    data_provider_conformance::run_tests(DataProviderTestConfig {
        manifest_url,
        bearer_token,
        expected_streamed_source_count: 1,
        expected_static_file_source_count: 0,
    })
}
