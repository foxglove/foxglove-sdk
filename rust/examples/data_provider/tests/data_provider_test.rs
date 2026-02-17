//! End-to-end tests for the data_provider example.
//!
//! This is a thin wrapper that starts the example binary as a subprocess, and delegates all checks
//! to the reusable test suite in [`example_data_provider`]'s library target.

use std::process::Stdio;
use std::time::Duration;

use example_data_provider::DataProviderTestConfig;

const BIND_ADDR: &str = "127.0.0.1:8080";

struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        self.0
            .kill()
            .expect("should be able to kill example_data_provider binary");
    }
}

/// Spawn the example binary and wait until it accepts connections.
fn start_server() -> KillOnDrop {
    let child = KillOnDrop(
        std::process::Command::new(env!("CARGO_BIN_EXE_example_data_provider"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("should be able to start example_data_provider binary"),
    );

    for _ in 0..100 {
        if std::net::TcpStream::connect(BIND_ADDR).is_ok() {
            return child;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("example_data_provider should become ready within 5 s");
}

fn main() {
    let manifest_url =
        format!("http://{BIND_ADDR}/v1/manifest?flightId=TEST123&startTime=2024-01-01T00:00:00Z&endTime=2024-01-01T00:00:05Z")
    .parse().unwrap();
    let _guard = start_server();

    example_data_provider::run_tests(DataProviderTestConfig {
        manifest_url,
        bearer_token: "test-token".into(),
        expected_streamed_source_count: 1,
        expected_static_file_source_count: 0,
    });
}
