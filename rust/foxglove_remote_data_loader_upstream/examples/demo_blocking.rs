//! Example showing how to use the upstream server SDK (blocking version).
//!
//! This example demonstrates:
//! - Implementing the [`UpstreamServerBlocking`] trait.
//! - The flow: declare channels, set manifest opts, stream data.
//! - Using [`generate_source_id`] to create unique IDs for caching.
//!
//! # Running the example
//!
//! ```sh
//! cargo run --example demo_blocking
//! ```
//!
//! # Testing the endpoints
//!
//! Get a manifest for a specific flight:
//! ```sh
//! curl "http://localhost:8080/v1/manifest?flightId=ABC123&startTime=2024-01-01T00:00:00Z&endTime=2024-01-02T00:00:00Z"
//! ```
//!
//! Stream MCAP data:
//! ```sh
//! curl "http://localhost:8080/v1/data?flightId=ABC123&startTime=2024-01-01T00:00:00Z&endTime=2024-01-02T00:00:00Z" --output data.mcap
//! ```
//!
//! Verify the MCAP file (requires mcap CLI):
//! ```sh
//! mcap info data.mcap
//! ```

use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use foxglove::FoxgloveError;
use serde::Deserialize;

use foxglove_remote_data_loader_upstream::{
    blocking, generate_source_id, AuthError, ManifestOpts, Url,
};

/// A simple upstream server.
struct BlockingExampleUpstream;

/// Query parameters for both manifest and data endpoints.
#[derive(Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
struct FlightParams {
    flight_id: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

impl blocking::UpstreamServer for BlockingExampleUpstream {
    type QueryParams = FlightParams;
    type Error = FoxgloveError;

    fn auth(&self, _bearer_token: Option<&str>, _params: &FlightParams) -> Result<(), AuthError> {
        // No authentication required for this demo
        Ok(())
    }

    fn build_source(
        &self,
        params: FlightParams,
        mut source: blocking::SourceBuilder<'_>,
    ) -> Result<(), FoxgloveError> {
        // Define our message type.
        #[derive(foxglove::Encode)]
        struct DemoMessage {
            msg: String,
            count: u32,
        }

        // 1. Declare channels.
        let channel = source.channel::<DemoMessage>("/demo");

        // 2. Set manifest metadata if this is a manifest request.
        if let Some(opts) = source.manifest() {
            *opts = ManifestOpts {
                id: generate_source_id("flight-data", 1, &params),
                name: format!("Flight {}", params.flight_id),
                start_time: params.start_time,
                end_time: params.end_time,
            };
        }

        // 3. Stream messages if this is a data request.
        let Some(mut handle) = source.into_stream_handle() else {
            return Ok(());
        };

        tracing::info!(flight_id = %params.flight_id, "streaming data");

        const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1MiB
        for i in 0..10 {
            let timestamp = params.start_time + chrono::Duration::milliseconds(i as i64 * 100);
            channel.log_with_time(
                &DemoMessage {
                    msg: format!("Data for flight {}", params.flight_id),
                    count: i,
                },
                timestamp.min(params.end_time),
            );

            if handle.buffer_size() >= MAX_BUFFER_SIZE {
                handle.flush()?;
            }
        }

        // Close the handle to finish the MCAP.
        handle.close()?;
        Ok(())
    }

    fn base_url(&self) -> Url {
        "http://localhost:8080".parse().unwrap()
    }
}

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%bind_address, "starting server");
    blocking::serve(BlockingExampleUpstream, bind_address)
}
