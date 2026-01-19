//! Demo example showing how to use the upstream server SDK (blocking version).
//!
//! This example demonstrates the fully synchronous API - no async code required!
//!
//! # Running the example
//!
//! ```sh
//! cargo run --example demo_blocking -p foxglove_remote_data_loader_upstream
//! ```
//!
//! # Testing the endpoints
//!
//! Get a manifest for a specific flight:
//! ```sh
//! curl "http://localhost:8080/v1/manifest?flightId=ABC123"
//! ```
//!
//! Stream MCAP data:
//! ```sh
//! curl "http://localhost:8080/v1/data?flightId=ABC123" --output data.mcap
//! ```

use std::{convert::Infallible, net::SocketAddr};

use chrono::{Duration, Utc};
use serde::Deserialize;

use foxglove_remote_data_loader_upstream::{
    generate_source_id, serve_blocking, AuthError, ManifestOpts, SourceBuilderBlocking,
    UpstreamServerBlocking, Url,
};

/// A simple upstream server using blocking I/O.
struct ExampleUpstreamBlocking;

/// Query parameters for both manifest and data endpoints.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlightParams {
    flight_id: String,
}

impl UpstreamServerBlocking for ExampleUpstreamBlocking {
    type QueryParams = FlightParams;
    type Error = Infallible;

    fn auth(&self, _bearer_token: Option<&str>, _params: &FlightParams) -> Result<(), AuthError> {
        // No authentication required for this demo
        Ok(())
    }

    fn build_source(
        &self,
        params: FlightParams,
        mut source: SourceBuilderBlocking<'_>,
    ) -> Result<(), Infallible> {
        // Define our message type
        #[derive(foxglove::Encode)]
        struct DemoMessage {
            msg: String,
            count: u32,
        }

        // 1. Declare channels
        let channel = source.channel::<DemoMessage>("/demo");

        // 2. Set manifest metadata
        if let Some(opts) = source.manifest() {
            let now = Utc::now();
            *opts = ManifestOpts {
                id: generate_source_id("flight-data", 1, &params.flight_id),
                name: format!("Flight {}", params.flight_id),
                start_time: now - Duration::hours(1),
                end_time: now,
            };
        }

        // 3. Stream data
        let Some(handle) = source.into_stream_handle() else {
            return Ok(());
        };

        // Log some demo data - all sync!
        println!("Streaming data for flight {}", params.flight_id);
        for i in 0..10 {
            channel.log(&DemoMessage {
                msg: format!("Data for flight {}", params.flight_id),
                count: i,
            });
        }

        // Finish the stream - sync!
        handle.finish().expect("finish stream");

        Ok(())
    }

    fn base_url(&self) -> Url {
        "http://localhost:8080".parse().unwrap()
    }
}

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    println!("Starting server on {bind_address}");
    serve_blocking(ExampleUpstreamBlocking, bind_address)
}
