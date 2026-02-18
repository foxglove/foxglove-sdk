//! CLI runner for the data provider conformance test suite.
//!
//! Connects to (or spawns) a data provider server and runs all conformance checks.
//!
//! # Environment variables
//!
//! - `DATA_PROVIDER_ADDR` (required) — socket address of the server, e.g. `127.0.0.1:8081`
//! - `DATA_PROVIDER_CMD` — path to a server binary to spawn; if unset, the server must already be
//!   running at `DATA_PROVIDER_ADDR`
//! - `DATA_PROVIDER_BEARER_TOKEN` — bearer token for authentication (default: `test-token`)
//!
//! # Examples
//!
//! Test a server that is already running:
//!
//! ```sh
//! DATA_PROVIDER_ADDR=127.0.0.1:8081 cargo run -p data_provider_conformance
//! ```
//!
//! Spawn a server binary and test it:
//!
//! ```sh
//! DATA_PROVIDER_CMD=cpp/build/example_data_provider \
//! DATA_PROVIDER_ADDR=127.0.0.1:8081 \
//!   cargo run -p data_provider_conformance
//! ```

use std::process::ExitCode;

use data_provider_conformance::DataProviderTestConfig;

fn main() -> ExitCode {
    let addr = std::env::var("DATA_PROVIDER_ADDR").unwrap_or_else(|_| {
        eprintln!("error: DATA_PROVIDER_ADDR environment variable must be set");
        eprintln!("  e.g. DATA_PROVIDER_ADDR=127.0.0.1:8081");
        std::process::exit(2);
    });

    let bearer_token =
        std::env::var("DATA_PROVIDER_BEARER_TOKEN").unwrap_or_else(|_| "test-token".into());

    // If DATA_PROVIDER_CMD is set, spawn the server binary and keep it alive for the test run.
    let _guard = std::env::var_os("DATA_PROVIDER_CMD")
        .map(|cmd| data_provider_conformance::spawn_server(cmd, &addr));

    let manifest_url = format!(
        "http://{addr}/v1/manifest\
         ?flightId=TEST123\
         &startTime=2024-01-01T00:00:00Z\
         &endTime=2024-01-01T00:00:05Z"
    )
    .parse()
    .unwrap_or_else(|e| {
        eprintln!("error: invalid address '{addr}': {e}");
        std::process::exit(2);
    });

    data_provider_conformance::run_tests(DataProviderTestConfig {
        manifest_url,
        bearer_token,
        expected_streamed_source_count: 1,
        expected_static_file_source_count: 0,
    })
}
