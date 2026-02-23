//! End-to-end tests for the data_provider example.
//!
//! This is a thin wrapper that starts the example binary as a subprocess, and delegates all checks
//! to the reusable test suite in [`data_provider_conformance`].

use std::process::ExitCode;

use data_provider_conformance::DataProviderTestConfig;

const BIND_ADDR: &str = "127.0.0.1:8080";

fn main() -> ExitCode {
    let _guard = data_provider_conformance::spawn_server(
        env!("CARGO_BIN_EXE_example_data_provider"),
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

    data_provider_conformance::run_tests(DataProviderTestConfig {
        manifest_url,
        expected_streamed_source_count: 1,
        expected_static_file_source_count: 0,
    })
}
