//! CLI runner for the remote data loader backend conformance test suite.
//!
//! Connects to (or spawns) a backend server and runs all conformance checks.
//!
//! # Environment variables
//!
//! - `REMOTE_DATA_LOADER_BACKEND_URL` (required) — full manifest URL including query parameters, e.g.
//!   `http://localhost:8081/v1/manifest?flightId=TEST&startTime=...&endTime=...`
//! - `REMOTE_DATA_LOADER_BACKEND_EXPECTED_STREAMED_SOURCE_COUNT` (required) — expected number of streamed
//!   sources in the manifest
//! - `REMOTE_DATA_LOADER_BACKEND_EXPECTED_STATIC_FILE_SOURCE_COUNT` (required) — expected number of static file
//!   sources in the manifest
//! - `REMOTE_DATA_LOADER_BACKEND_CMD` — path to a server binary to spawn; if unset, the server must already be
//!   running at the host/port in `REMOTE_DATA_LOADER_BACKEND_URL`
//!
//! # Examples
//!
//! Test a server that is already running:
//!
//! ```sh
//! REMOTE_DATA_LOADER_BACKEND_URL="http://localhost:8081/v1/manifest?flightId=TEST&startTime=2024-01-01T00:00:00Z&endTime=2024-01-01T00:00:05Z" \
//! REMOTE_DATA_LOADER_BACKEND_EXPECTED_STREAMED_SOURCE_COUNT=1 \
//! REMOTE_DATA_LOADER_BACKEND_EXPECTED_STATIC_FILE_SOURCE_COUNT=0 \
//!   cargo run -p remote_data_loader_backend_conformance
//! ```
//!
//! Spawn a server binary and test it:
//!
//! ```sh
//! REMOTE_DATA_LOADER_BACKEND_CMD=cpp/build/example_remote_data_loader_backend \
//! REMOTE_DATA_LOADER_BACKEND_URL="http://localhost:8081/v1/manifest?flightId=TEST&startTime=2024-01-01T00:00:00Z&endTime=2024-01-01T00:00:05Z" \
//! REMOTE_DATA_LOADER_BACKEND_EXPECTED_STREAMED_SOURCE_COUNT=1 \
//! REMOTE_DATA_LOADER_BACKEND_EXPECTED_STATIC_FILE_SOURCE_COUNT=0 \
//!   cargo run -p remote_data_loader_backend_conformance
//! ```

use std::process::ExitCode;

use remote_data_loader_backend_conformance::{RemoteDataLoaderBackendTestConfig, Url};

fn var(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(s) => Some(s),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(s)) => {
            panic!("{name} must be valid Unicode, but got {s:?}")
        }
    }
}

fn main() -> ExitCode {
    let manifest_url: Url = var("REMOTE_DATA_LOADER_BACKEND_URL")
        .expect("REMOTE_DATA_LOADER_BACKEND_URL must be set")
        .parse()
        .expect("REMOTE_DATA_LOADER_BACKEND_URL must be a valid URL");
    assert_eq!(
        manifest_url.scheme(),
        "http",
        "REMOTE_DATA_LOADER_BACKEND_URL must use http://"
    );

    let expected_streamed_source_count: usize = var(
        "REMOTE_DATA_LOADER_BACKEND_EXPECTED_STREAMED_SOURCE_COUNT",
    )
    .expect("REMOTE_DATA_LOADER_BACKEND_EXPECTED_STREAMED_SOURCE_COUNT must be set")
    .parse()
    .expect(
        "REMOTE_DATA_LOADER_BACKEND_EXPECTED_STREAMED_SOURCE_COUNT must be a nonnegative integer",
    );
    let expected_static_file_source_count: usize =
        var("REMOTE_DATA_LOADER_BACKEND_EXPECTED_STATIC_FILE_SOURCE_COUNT")
            .expect("REMOTE_DATA_LOADER_BACKEND_EXPECTED_STATIC_FILE_SOURCE_COUNT must be set")
            .parse()
            .expect(
                "REMOTE_DATA_LOADER_BACKEND_EXPECTED_STATIC_FILE_SOURCE_COUNT must be a nonnegative integer",
            );

    // If REMOTE_DATA_LOADER_BACKEND_CMD is set, spawn the server binary and keep it alive for the test run.
    // The socket address to wait on is derived from REMOTE_DATA_LOADER_BACKEND_URL.
    let _guard = std::env::var_os("REMOTE_DATA_LOADER_BACKEND_CMD").map(|cmd| {
        let host = manifest_url
            .host_str()
            .expect("every valid http:// URL has a host");
        let port = manifest_url
            .port_or_known_default()
            .expect("default is known for http://");
        let addr = format!("{host}:{port}");
        remote_data_loader_backend_conformance::spawn_server(cmd, &addr)
    });

    remote_data_loader_backend_conformance::run_tests(RemoteDataLoaderBackendTestConfig {
        manifest_url,
        expected_streamed_source_count,
        expected_static_file_source_count,
    })
}
