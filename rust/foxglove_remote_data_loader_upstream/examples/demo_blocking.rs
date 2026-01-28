//! Example showing how to use the upstream server SDK (blocking version).
//!
//! This example demonstrates:
//! - Implementing the [`blocking::UpstreamServer`] trait.
//! - The flow: auth, initialize, metadata, stream.
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
use foxglove::Channel;
use serde::Deserialize;

use foxglove_remote_data_loader_upstream::{
    blocking, generate_source_id, AuthError, BoxError, ChannelRegistry, Metadata,
};

/// Define our message type.
#[derive(foxglove::Encode)]
struct DemoMessage {
    msg: String,
    count: u32,
}

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

/// Context holding channels and shared state.
struct FlightContext {
    params: FlightParams,
    demo: Channel<DemoMessage>,
}

impl blocking::UpstreamServer for BlockingExampleUpstream {
    type QueryParams = FlightParams;
    type Context = FlightContext;

    fn auth(&self, _bearer_token: Option<&str>, _params: &FlightParams) -> Result<(), AuthError> {
        // No authentication required for this demo
        Ok(())
    }

    fn initialize(
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

    fn metadata(&self, ctx: FlightContext) -> Result<Metadata, BoxError> {
        Ok(Metadata {
            id: generate_source_id("flight-data", 1, &ctx.params),
            name: format!("Flight {}", ctx.params.flight_id),
            start_time: ctx.params.start_time,
            end_time: ctx.params.end_time,
        })
    }

    fn stream(
        &self,
        ctx: FlightContext,
        mut handle: blocking::StreamHandle,
    ) -> Result<(), BoxError> {
        tracing::info!(flight_id = %ctx.params.flight_id, "streaming data");

        const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1MiB
        for i in 0..10 {
            let timestamp =
                ctx.params.start_time + chrono::Duration::milliseconds(i as i64 * 100);
            ctx.demo.log_with_time(
                &DemoMessage {
                    msg: format!("Data for flight {}", ctx.params.flight_id),
                    count: i,
                },
                timestamp.min(ctx.params.end_time),
            );

            if handle.buffer_size() >= MAX_BUFFER_SIZE {
                handle.flush()?;
            }
        }

        // Close the handle to finish the MCAP.
        handle.close()?;
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%bind_address, "starting server");
    blocking::serve(BlockingExampleUpstream, bind_address)
}
