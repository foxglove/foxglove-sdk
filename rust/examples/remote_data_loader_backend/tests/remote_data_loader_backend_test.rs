//! End-to-end tests for the remote_data_loader_backend example.
//!
//! This is a thin wrapper that starts the example binary as a subprocess, and delegates all checks
//! to the reusable test suite in [`remote_data_loader_backend_conformance`].

use std::process::ExitCode;

use remote_data_loader_backend_conformance::RemoteDataLoaderBackendTestConfig;

const BIND_ADDR: &str = "127.0.0.1:8080";

fn main() -> ExitCode {
    let _guard = remote_data_loader_backend_conformance::spawn_server(
        env!("CARGO_BIN_EXE_example_remote_data_loader_backend"),
        BIND_ADDR,
    );

    let manifest_url = format!(
        "http://{BIND_ADDR}/v1/manifest\
         ?flightId=TEST123\
         &startTime=2024-01-01T00:00:00Z\
         &endTime=2024-01-01T00:00:05Z"
    )
    .parse()
    .unwrap();

    remote_data_loader_backend_conformance::run_tests(RemoteDataLoaderBackendTestConfig {
        manifest_url,
        expected_streamed_source_count: 1,
        expected_static_file_source_count: 0,
    })
}
