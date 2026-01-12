//! Demo example showing how to use the upstream server SDK.
//!
//! This example demonstrates:
//! - Implementing `Authenticator`, `ManifestProvider`, and `DataProvider` traits
//! - Using query parameters with proper camelCase serialization
//! - Streaming MCAP data with proper error handling
//! - Request tracing for observability
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
//! Stream MCAP data for a specific source:
//! ```sh
//! curl "http://localhost:8080/v1/data?sourceId=ABC123" --output data.mcap
//! ```
//!
//! Verify the MCAP file (requires mcap CLI):
//! ```sh
//! mcap info data.mcap
//! ```

#![allow(refining_impl_trait)]

use std::{convert::Infallible, net::SocketAddr};

use axum::body::Bytes;
use chrono::Utc;
use foxglove::stream::create_mcap_stream;
use futures::StreamExt;
use serde::Deserialize;
use url::Url;

use foxglove_remote_data_loader_upstream::{
    manifest::{Manifest, StreamedSource, Topic, UpstreamSource},
    serve, AuthenticationError, Authenticator, AuthorizationError, DataProvider, ManifestProvider,
};

/// A simple message type that will be encoded in the MCAP stream.
#[derive(foxglove::Encode)]
struct DemoMessage {
    msg: String,
    count: u32,
}

/// Query parameters for the manifest endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestParams {
    flight_id: String,
}

/// Query parameters for the data endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataParams {
    source_id: String,
}

/// A simple provider that serves both manifest and data endpoints.
struct DemoProvider;

// Implement authentication with no-auth (public access).
// This is a conscious choice - the SDK requires you to think about auth.
impl Authenticator for DemoProvider {
    type Identity = ();

    async fn authenticate(
        &self,
        _bearer_token: Option<&str>,
    ) -> Result<Self::Identity, AuthenticationError> {
        // No authentication required - always succeed
        Ok(())
    }
}

impl ManifestProvider for DemoProvider {
    type QueryParams = ManifestParams;

    async fn get_manifest(
        &self,
        _identity: Self::Identity,
        params: Self::QueryParams,
    ) -> Result<Manifest, Infallible> {
        tracing::info!(flight_id = %params.flight_id, "serving manifest");

        // Build the data URL with proper query parameter escaping
        let mut data_url = Url::parse("http://localhost:8080/v1/data").unwrap();
        data_url
            .query_pairs_mut()
            .append_pair("sourceId", &params.flight_id);

        let now = Utc::now();
        Ok(Manifest {
            name: Some(format!("Flight {}", params.flight_id)),
            sources: vec![UpstreamSource::Streamed(StreamedSource {
                url: data_url,
                id: Some(format!("flight-{}", params.flight_id)),
                topics: vec![Topic {
                    name: "/demo".to_string(),
                    message_encoding: "protobuf".to_string(),
                    schema_id: None,
                }],
                schemas: vec![],
                start_time: now,
                end_time: now,
            })],
        })
    }
}

impl DataProvider for DemoProvider {
    type QueryParams = DataParams;

    async fn authorize_data(
        &self,
        _identity: Self::Identity,
        _query_params: &Self::QueryParams,
    ) -> Result<(), AuthorizationError> {
        // Always allow access
        Ok(())
    }

    async fn stream_data(
        &self,
        params: Self::QueryParams,
    ) -> Result<impl futures::Stream<Item = Result<Bytes, Infallible>> + Send + 'static, Infallible>
    {
        tracing::info!(source_id = %params.source_id, "streaming data");

        let (mut handle, stream) = create_mcap_stream();

        // Create a channel for our demo messages
        let channel = handle.channel_builder("/demo").build::<DemoMessage>();
        let source_id = params.source_id;

        // Spawn a task to write messages to the stream
        tokio::spawn(async move {
            for i in 0..10 {
                channel.log(&DemoMessage {
                    msg: format!("Data for source {source_id}"),
                    count: i,
                });
                // Flush after each message to push bytes to the stream
                if let Err(e) = handle.flush().await {
                    tracing::warn!(error = %e, "failed to flush mcap stream");
                }
            }
            // Close the handle to finalize the MCAP stream
            if let Err(e) = handle.close().await {
                tracing::warn!(error = %e, "failed to close mcap stream");
            }
        });

        // Map Stream<Item = Bytes> to Stream<Item = Result<Bytes, Infallible>>
        Ok(stream.map(Ok))
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let addr: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%addr, "starting server");
    serve(DemoProvider, addr).await
}
