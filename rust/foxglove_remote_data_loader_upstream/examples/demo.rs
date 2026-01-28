//! Example showing how to use the upstream server SDK (async version).
//!
//! This example demonstrates:
//! - Implementing the [`UpstreamServer`] trait.
//! - The flow: auth, initialize, metadata, stream.
//!
//! # Running the example
//!
//! ```sh
//! cargo run --example demo
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
use foxglove::Channel;
use serde::Deserialize;

use foxglove_remote_data_loader_upstream::{
    serve, AuthError, BoxError, ChannelRegistry, Metadata, StreamHandle, UpstreamServer,
};

/// Define our message type.
#[derive(foxglove::Encode)]
struct DemoMessage {
    msg: String,
    count: u32,
}

/// A simple upstream server.
struct ExampleUpstream;

/// Query parameters for both manifest and data endpoints.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlightParams {
    flight_id: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

impl FlightParams {
    fn slug(&self) -> String {
        format!("{}-{}-{}", self.flight_id, self.start_time, self.end_time)
    }
}

/// Context holding channels and shared state.
struct FlightContext {
    params: FlightParams,
    demo: Channel<DemoMessage>,
}

impl UpstreamServer for ExampleUpstream {
    type QueryParams = FlightParams;
    type Context = FlightContext;

    async fn auth(
        &self,
        _bearer_token: Option<&str>,
        _params: &FlightParams,
    ) -> Result<(), AuthError> {
        // No authentication required for this demo
        Ok(())
    }

    async fn initialize(
        &self,
        params: FlightParams,
        reg: &mut impl ChannelRegistry,
    ) -> Result<FlightContext, BoxError> {
        // Declare channels and build context
        Ok(FlightContext {
            params,
            demo: reg.channel("/demo"),
        })
    }

    async fn metadata(&self, ctx: FlightContext) -> Result<Metadata, BoxError> {
        Ok(Metadata {
            // Stable identifier for caching - include all params that affect output
            id: format!("flight-v1-{}", ctx.params.slug()),
            name: format!("Flight {}", ctx.params.flight_id),
            start_time: ctx.params.start_time,
            end_time: ctx.params.end_time,
        })
    }

    async fn stream(&self, ctx: FlightContext, mut handle: StreamHandle) -> Result<(), BoxError> {
        tracing::info!(flight_id = %ctx.params.flight_id, "streaming data");

        const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1MiB
        for i in 0..10 {
            let timestamp = ctx.params.start_time + chrono::Duration::milliseconds(i as i64 * 100);
            ctx.demo.log_with_time(
                &DemoMessage {
                    msg: format!("Data for flight {}", ctx.params.flight_id),
                    count: i,
                },
                timestamp.min(ctx.params.end_time),
            );

            if handle.buffer_size() >= MAX_BUFFER_SIZE {
                handle.flush().await?;
            }
        }

        // Close the handle to finish the MCAP.
        handle.close().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%bind_address, "starting server");
    serve(ExampleUpstream, bind_address).await
}
