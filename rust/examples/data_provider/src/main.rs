//! Example showing how to implement a Foxglove data provider using axum directly.
//!
//! This implements the two endpoints required by the HTTP API:
//! - `GET /v1/manifest` - returns a JSON manifest describing the available data
//! - `GET /v1/data` - streams MCAP data
//!
//! # Running the example
//!
//! See the remote data loader local development guide to test this properly in the Foxglove app.
//!
//! You can also test basic functionality with curl:
//!
//! To run the example server:
//!
//! ```sh
//! cargo run -p example_data_provider
//! ```
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

use example_data_provider::app;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%bind_address, "starting server");

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app()).await
}
