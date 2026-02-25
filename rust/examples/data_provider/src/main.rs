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

use std::convert::Infallible;
use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, DurationRound, Utc};
use foxglove::stream::create_mcap_stream;
use foxglove::{
    data_provider::{ChannelSet, Manifest, StreamedSource, UpstreamSource},
    schemas::Vector3,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

// Routes for the endpoints. The specific values are not part of the API; you can change them to
// whatever you want.
const MANIFEST_ROUTE: &str = "/v1/manifest";
const DATA_ROUTE: &str = "/v1/data";

/// Specification of what to load.
///
/// Deserialized from the query parameters in the incoming HTTP request.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlightParams {
    flight_id: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
}

/// Check the bearer token to see if the user is authorized to access the flight.
fn check_auth(_headers: &HeaderMap, _params: &FlightParams) -> Result<(), StatusCode> {
    // EXAMPLE ONLY: REPLACE THIS WITH A REAL AUTH CHECK.
    Ok(())
}

/// Handler for `GET /v1/manifest`.
///
/// Builds a manifest describing the channels and schemas available for the requested flight.
///
/// The user **MUST** be authorized to read all sources returned in the manifest. Do not rely
/// on authorization checks on individual sources, because they may not be called for cached data.
async fn manifest_handler(headers: HeaderMap, Query(params): Query<FlightParams>) -> Response {
    if let Err(status) = check_auth(&headers, &params) {
        return status.into_response();
    }

    // Declare a single channel of Foxglove `Vector3` messages on topic "/demo".
    let mut channels = ChannelSet::new();
    channels.insert::<Vector3>("/demo");
    let (topics, schemas) = channels.into_topics_and_schemas();

    let query = serde_urlencoded::to_string(&params).unwrap();
    let source = StreamedSource {
        // We're providing the data from this service in this example, but in principle this could
        // be any URL.
        url: format!("{DATA_ROUTE}?{query}"),
        // `id` must unique to this data source. Otherwise, incorrect data may be served from cache.
        //
        // Here we reuse the query string to make sure we don't forget any parameters. We also
        // include a version number we increment whenever we change the data handler.
        id: Some(format!("flight-v1-{query}")),
        topics,
        schemas,
        start_time: params.start_time,
        end_time: params.end_time,
    };

    let manifest = Manifest {
        name: Some(format!("Flight {}", params.flight_id)),
        sources: vec![UpstreamSource::Streamed(source)],
    };

    Json(manifest).into_response()
}

/// Handler for `GET /v1/data`.
///
/// Streams MCAP data for the requested flight. The response body is a stream of MCAP bytes.
async fn data_handler(headers: HeaderMap, Query(params): Query<FlightParams>) -> Response {
    if let Err(status) = check_auth(&headers, &params) {
        return status.into_response();
    }

    let (mut handle, mcap_stream) = create_mcap_stream();

    // Declare channels.
    let channel = handle.channel_builder("/demo").build::<Vector3>();

    // Spawn a task to stream data asynchronously rather than buffering it all up front.
    tokio::spawn(async move {
        // In this example, we query a simulated dataset, but in a real implementation you would
        // probably query a database or other storage.
        //
        // This simulated dataset consists of messages emitted every second from the Unix epoch.
        tracing::info!(flight_id = %params.flight_id, "streaming data");

        // Generate one message per second.
        const INTERVAL: chrono::TimeDelta = chrono::TimeDelta::seconds(1);

        // Compute timestamp of first message by rounding the start time up to the second.
        let mut ts = params
            .start_time
            .max(DateTime::UNIX_EPOCH) // Ignore negative start times.
            .duration_round_up(INTERVAL)
            .ok();
        while let Some(inner) = ts.filter(|inner| *inner <= params.end_time) {
            // Messages in the output MUST appear in ascending timestamp order. Otherwise, playback
            // will be incorrect.
            //
            // `log_with_time()` immediately writes messages to the output MCAP, so you MUST NOT
            // call it with out-of-order timestamps, even across different channels.
            channel.log_with_time(
                &Vector3 {
                    x: inner.timestamp() as f64,
                    y: 0.0,
                    z: 0.0,
                },
                inner,
            );

            // Periodically flush buffered data to the response stream. This serves two purposes:
            // the client receives data incrementally instead of all at once, and memory usage stays
            // bounded instead of growing with the entire recording.
            const FLUSH_THRESHOLD: usize = 1024 * 1024;
            if handle.buffer_size() >= FLUSH_THRESHOLD {
                if let Err(e) = handle.flush().await {
                    tracing::error!(%e, "flush failed");
                    return;
                }
            }

            ts = inner.checked_add_signed(INTERVAL);
        }

        // Finalize the streamed MCAP and ensure it is sent to the client.
        if let Err(e) = handle.close().await {
            tracing::error!(%e, "error closing MCAP stream");
        }
    });

    Body::from_stream(mcap_stream.map(Ok::<_, Infallible>)).into_response()
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route(MANIFEST_ROUTE, get(manifest_handler))
        .route(DATA_ROUTE, get(data_handler));

    let bind_address: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    tracing::info!(%bind_address, "starting server");

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app).await
}
