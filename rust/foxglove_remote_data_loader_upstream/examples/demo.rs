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
//! curl "http://localhost:8080/v1/manifest?flightId=ABC123"
//! ```
//!
//! Stream MCAP data:
//! ```sh
//! curl "http://localhost:8080/v1/data?flightId=ABC123" --output data.mcap
//! ```
//!
//! Verify the MCAP file (requires mcap CLI):
//! ```sh
//! mcap info data.mcap
//! ```

use std::{convert::Infallible, net::SocketAddr};

use chrono::{Duration, Utc};
use serde::Deserialize;

use foxglove_remote_data_loader_upstream::{
    generate_source_id, serve, AuthError, ManifestOpts, SourceBuilder, UpstreamServer, Url,
};

/// A simple upstream server that serves both manifest and data endpoints.
struct ExampleUpstream;

/// Query parameters for both manifest and data endpoints.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlightParams {
    flight_id: String,
}

impl UpstreamServer for ExampleUpstream {
    type QueryParams = FlightParams;
    type Error = Infallible;

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
    ) -> Result<(), Infallible> {
        // Define our message type
        #[derive(foxglove::Encode)]
        struct DemoMessage {
            msg: String,
            count: u32,
        }

        // 1. Declare channels (must be done before manifest/stream)
        let channel = source.channel::<DemoMessage>("/demo");

        // 2. Set manifest metadata (only runs for manifest requests)
        if let Some(opts) = source.manifest() {
            let now = Utc::now();
            *opts = ManifestOpts {
                id: generate_source_id("flight-data", 1, &params.flight_id),
                name: format!("Flight {}", params.flight_id),
                start_time: now - Duration::hours(1),
                end_time: now,
            };
        }

        // 3. Stream data (only runs for data requests)
        let Some(handle) = source.into_stream_handle() else {
            // Manifest request - we're done
            return Ok(());
        };

        // Log some demo data
        tracing::info!(flight_id = %params.flight_id, "streaming data");
        for i in 0..10 {
            channel.log(&DemoMessage {
                msg: format!("Data for flight {}", params.flight_id),
                count: i,
            });
        }

        // Finish the stream (flushes all data)
        handle.finish().await.expect("finish stream");

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
