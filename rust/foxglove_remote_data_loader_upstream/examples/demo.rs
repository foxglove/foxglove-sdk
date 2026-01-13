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

use std::{convert::Infallible, net::SocketAddr};

use axum::body::Bytes;
use chrono::{DateTime, Utc};
use foxglove::{stream::create_mcap_stream, FoxgloveError};
use futures::StreamExt;
use serde::Deserialize;
use url::Url;

use foxglove_remote_data_loader_upstream::{
    manifest::{Manifest, StreamedSource, Topic, UpstreamSource},
    serve, AuthenticationError, Authenticator, AuthorizationError, DataProvider, ManifestProvider,
    DATA_ROUTE,
};

/// A simple remote data loader upstream that serves both manifest and data endpoints.
struct ExampleUpstream {
    base_url: Url,
}

// You always have to implement `Authenticator`. If you don't want authentication, you can use a
// dummy implementation like this one.
impl Authenticator for ExampleUpstream {
    type Identity = ();

    async fn authenticate(
        &self,
        _bearer_token: Option<&str>,
    ) -> Result<Self::Identity, AuthenticationError> {
        // This is just a no-auth example, so always succeed.
        Ok(())
    }
}

/// Query parameters for the manifest endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestParams {
    flight_id: String,
}

impl ManifestProvider for ExampleUpstream {
    type QueryParams = ManifestParams;
    type Error = Infallible;

    async fn get_manifest(
        &self,
        _identity: Self::Identity,
        Self::QueryParams { flight_id }: Self::QueryParams,
    ) -> Result<Manifest, Infallible> {
        tracing::info!(%flight_id, "serving manifest");

        // Build the data URL with proper query parameter escaping
        let mut data_url = self.base_url.join(DATA_ROUTE).unwrap();
        data_url
            .query_pairs_mut()
            .append_pair("sourceId", &flight_id);

        Ok(Manifest {
            name: Some(format!("Flight {}", flight_id)),
            sources: vec![UpstreamSource::Streamed(StreamedSource {
                url: data_url,
                id: Some(format!("flight-{}", flight_id)),
                topics: vec![Topic {
                    name: "/demo".to_string(),
                    message_encoding: "protobuf".to_string(),
                    schema_id: None,
                }],
                schemas: vec![],
                start_time: DateTime::<Utc>::MIN_UTC,
                end_time: DateTime::<Utc>::MAX_UTC,
            })],
        })
    }
}

/// Query parameters for the data endpoint.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataParams {
    source_id: String,
}

impl DataProvider for ExampleUpstream {
    type QueryParams = DataParams;
    type StreamInitError = Infallible;
    type StreamDataError = FoxgloveError;

    async fn authorize_data(
        &self,
        _identity: Self::Identity,
        _query_params: &Self::QueryParams,
    ) -> Result<(), AuthorizationError> {
        // Always allow access.
        Ok(())
    }

    async fn stream_data(
        &self,
        Self::QueryParams { source_id }: Self::QueryParams,
    ) -> Result<
        impl futures::Stream<Item = Result<Bytes, Self::StreamDataError>> + Send + 'static,
        Self::StreamInitError,
    > {
        #[derive(foxglove::Encode)]
        struct DemoMessage {
            msg: String,
            count: u32,
        }

        tracing::info!(%source_id, "streaming data");

        // Create a single-channel MCAP and write some messages to it.
        let (handle, stream) = create_mcap_stream();
        let channel = handle.channel_builder("/demo").build::<DemoMessage>();

        // Send messages from a different task so we can return the stream without building up
        // the whole MCAP in memory. If you use blocking I/O, use `spawn_blocking` instead.
        let join_handle = tokio::spawn(async move {
            for i in 0..10 {
                channel.log(&DemoMessage {
                    msg: format!("Data for source {source_id}"),
                    count: i,
                });
            }
            // Dropping `handle` will finalize the MCAP and flush buffers, but errors will be
            // ignored. Call `close()` explicitly to check for errors.
            handle.close().await.inspect_err(|error| {
                tracing::warn!(%error, "failed to close mcap stream");
            })?;

            Ok::<(), Self::StreamDataError>(())
        });

        // If the task finishes with an error, append it to the stream.
        let error_stream = futures::stream::once(join_handle).filter_map(|result| async move {
            match result.expect("task should not panic") {
                Ok(()) => None,
                Err(error) => Some(Err(error)),
            }
        });
        // `stream` is a `Stream<Item = Bytes>`, but we need a `Stream<Item = Result<Bytes,
        // Self::StreamDataError>>`, so just wrap each item with `Ok`.
        Ok(stream.map(Ok).chain(error_stream))
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let base_url: Url = "http://localhost:8080".parse().unwrap();
    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%bind_address, "starting server");
    serve(ExampleUpstream { base_url }, bind_address).await
}
