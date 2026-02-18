//! CLI runner for the data provider conformance test suite.
//!
//! Connects to (or spawns) a data provider server and runs all conformance checks.
//!
//! # Environment variables
//!
//! - `DATA_PROVIDER_URL` (required) — full manifest URL including query parameters,
//!   e.g. `http://127.0.0.1:8081/v1/manifest?flightId=TEST&startTime=...&endTime=...`
//! - `DATA_PROVIDER_CMD` — path to a server binary to spawn; if unset, the server must already be
//!   running at the host/port in `DATA_PROVIDER_URL`
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
//! DATA_PROVIDER_URL="http://127.0.0.1:8081/v1/manifest?flightId=TEST&startTime=2024-01-01T00:00:00Z&endTime=2024-01-01T00:00:05Z" \
//!   cargo run -p data_provider_conformance
//! ```

use std::process::ExitCode;

use data_provider_conformance::{DataProviderTestConfig, Url};

fn main() -> ExitCode {
    let manifest_url: Url = std::env::var("DATA_PROVIDER_URL")
        .expect("DATA_PROVIDER_URL must be set")
        .parse()
        .expect("DATA_PROVIDER_URL must be a valid URL");

    let bearer_token =
        std::env::var("DATA_PROVIDER_BEARER_TOKEN").unwrap_or_else(|_| "test-token".into());

    // If DATA_PROVIDER_CMD is set, spawn the server binary and keep it alive for the test run.
    // The socket address to wait on is derived from DATA_PROVIDER_URL.
    let _guard = std::env::var_os("DATA_PROVIDER_CMD").map(|cmd| {
        let host = manifest_url
            .host_str()
            .expect("DATA_PROVIDER_URL must have a host");
        let port = manifest_url
            .port_or_known_default()
            .expect("DATA_PROVIDER_URL must have a port");
        let addr = format!("{host}:{port}");
        data_provider_conformance::spawn_server(cmd, &addr)
    });

    data_provider_conformance::run_tests(DataProviderTestConfig {
        manifest_url,
        bearer_token,
        expected_streamed_source_count: 1,
        expected_static_file_source_count: 0,
    })
}
