//! Example showing how to use the upstream server SDK (blocking version).
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
    blocking, AuthError, BoxError, ChannelRegistry, Metadata,
};

/// A simple message type for this example.
#[derive(foxglove::Encode)]
struct DemoMessage {
    msg: String,
    count: u32,
}

/// A simple upstream server.
///
/// This is empty in this simple example, but it could be used to hold a database connection or
/// other state shared across all requests.
struct BlockingExampleUpstream;

/// Specification of what to load.
///
/// This is deserialized from the query parameters in the incoming HTTP request.
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

/// Context holding request-specific state.
///
/// This should always contain the requested channels. It may also contain query parameters or other
/// information needed by both the `metadata` and `stream` methods.
struct FlightContext {
    params: FlightParams,
    demo: Channel<DemoMessage>,
}

impl blocking::UpstreamServer for BlockingExampleUpstream {
    type QueryParams = FlightParams;
    type Context = FlightContext;

    fn auth(&self, _bearer_token: Option<&str>, _params: &FlightParams) -> Result<(), AuthError> {
        // No authentication in this demo.
        Ok(())
    }

    fn initialize(
        &self,
        params: FlightParams,
        reg: &mut impl ChannelRegistry,
    ) -> Result<FlightContext, BoxError> {
        // Declare a channel for our demo messages and store the query parameters for later. This
        // is passed verbatim to `Self::metadata()` and `Self::stream()`.
        Ok(FlightContext {
            params,
            demo: reg.channel("/demo"),
        })
    }

    fn metadata(&self, ctx: FlightContext) -> Result<Metadata, BoxError> {
        Ok(Metadata {
            // Stable identifier for caching - include all params that affect output
            id: format!("flight-v1-{}", ctx.params.slug()),
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

        for i in 0..10 {
            let timestamp = ctx.params.start_time + chrono::Duration::milliseconds(i as i64 * 100);
            ctx.demo.log_with_time(
                &DemoMessage {
                    msg: format!("Data for flight {}", ctx.params.flight_id),
                    count: i,
                },
                timestamp.min(ctx.params.end_time),
            );

            // Regularly flush the buffer to ensure messages are not buffered indefinitely. You
            // should adjust this based on your message size, network bandwidth and latency
            // requirements.
            const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1MiB
            if handle.buffer_size() >= MAX_BUFFER_SIZE {
                handle.flush()?;
            }
        }

        // Close the handle to finish the MCAP and send the final buffer.
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
