//! End-to-end tests for the data_provider example.
//!
//! This is a thin wrapper that starts the example binary and delegates all
//! checks to the reusable [`data_provider_tests`] crate. The child process is
//! spawned with [`tokio::process::Command::kill_on_drop`] so it is killed
//! reliably on both normal return and panic unwinding.

use std::net::TcpStream;
use std::process::Stdio;
use std::time::Duration;

use data_provider_tests::DataProviderTestConfig;

const BASE_URL: &str = "http://127.0.0.1:8080";
const BIND_ADDR: &str = "127.0.0.1:8080";

/// A running server whose child process is killed on drop.
struct Server {
    _child: tokio::process::Child,
    _runtime: tokio::runtime::Runtime,
}

/// Spawn the example binary and wait until it accepts connections.
fn start_server() -> Server {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("should be able to build tokio runtime");

    let child = runtime.block_on(async {
        tokio::process::Command::new(env!("CARGO_BIN_EXE_example_data_provider"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("should be able to start example_data_provider binary")
    });

    for _ in 0..100 {
        if TcpStream::connect(BIND_ADDR).is_ok() {
            return Server {
                _child: child,
                _runtime: runtime,
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("example_data_provider should become ready within 5 s");
}

fn main() {
    let _server = start_server();

    data_provider_tests::run(&DataProviderTestConfig {
        base_url: BASE_URL.into(),
        manifest_url: format!(
            "{BASE_URL}/v1/manifest?flightId=TEST123\
             &startTime=2024-01-01T00:00:00Z\
             &endTime=2024-01-01T00:00:05Z"
        ),
        bearer_token: "test-token".into(),
    });
}
