//! Demo example showing how to use the upstream server SDK (async version).
//!
//! This example demonstrates:
//! - Implementing the [`UpstreamServer`] trait
//! - The linear flow: declare channels → set manifest opts → stream data
//! - Using [`generate_source_id`] for cache-safe IDs
//!
//! # Running the example
//!
//! ```sh
//! cargo run --example demo -p foxglove_remote_data_loader_upstream
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
    generate_source_id, serve, AuthError, ManifestOpts, SourceBuilder, UpstreamServer, Url,
};

/// A simple upstream server that serves both manifest and data endpoints.
struct ExampleUpstream;

/// Query parameters for both manifest and data endpoints.
#[derive(Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
struct FlightParams {
    flight_id: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

impl UpstreamServer for ExampleUpstream {
    type QueryParams = FlightParams;
    type Error = FoxgloveError;

    async fn auth(
        &self,
        _bearer_token: Option<&str>,
        _params: &FlightParams,
    ) -> Result<(), AuthError> {
        // No authentication required for this demo
        Ok(())
    }

    async fn build_source(
        &self,
        params: FlightParams,
        mut source: SourceBuilder<'_>,
    ) -> Result<(), FoxgloveError> {
        // Define our message type
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
            channel.log(&DemoMessage {
                msg: format!("Data for flight {}", params.flight_id),
                count: i,
            });

            if handle.buffer_size() >= MAX_BUFFER_SIZE {
                handle.flush().await?;
            }
        }

        // Close the handle to finish the MCAP.
        handle.close().await?;

        Ok(())
    }

    fn base_url(&self) -> Url {
        "http://localhost:8080".parse().unwrap()
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%bind_address, "starting server");
    serve(ExampleUpstream, bind_address).await
}
