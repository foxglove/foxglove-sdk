//! Example showing how to use the upstream server SDK with CSV files.
//!
//! This example demonstrates:
//! - Loading a CSV file from disk.
//! - Extracting timestamps from a column.
//! - Streaming each row as key/value pairs.
//!
//! # Running the example
//!
//! ```sh
//! cargo run --example csv_loader
//! ```
//!
//! # Testing the endpoints
//!
//! Get a manifest for the bundled CSV file:
//! ```sh
//! curl "http://localhost:8080/v1/manifest?csvPath=examples/data/sample.csv"
//! ```
//!
//! Stream MCAP data:
//! ```sh
//! curl "http://localhost:8080/v1/data?csvPath=examples/data/sample.csv" --output data.mcap
//! ```
//!
//! # Optional query parameters
//! - `topic`: the output topic (defaults to `/csv`)
//! - `timestampColumn`: column name (defaults to `timestamp`)
//! - `timestampFormat`: one of `rfc3339`, `unix_seconds`, `unix_millis`,
//!   `unix_micros`, `unix_nanos`
use std::net::SocketAddr;

use chrono::{DateTime, TimeZone, Utc};
use foxglove::schemas::KeyValuePair;
use serde::Deserialize;

use foxglove_remote_data_loader_upstream::{
    generate_source_id, serve, AuthError, BoxError, ManifestOpts, MaybeChannel, SourceBuilder,
    StreamHandle, UpstreamServer, Url,
};

const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1MiB

/// A simple upstream server for CSV files.
struct CsvUpstream;

/// Query parameters for both manifest and data endpoints.
#[derive(Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
struct CsvParams {
    csv_path: String,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default = "default_timestamp_column")]
    timestamp_column: String,
    #[serde(default)]
    timestamp_format: TimestampFormat,
}

#[derive(Clone, Copy, Debug, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
enum TimestampFormat {
    Rfc3339,
    UnixSeconds,
    UnixMillis,
    UnixMicros,
    UnixNanos,
}

impl Default for TimestampFormat {
    fn default() -> Self {
        Self::Rfc3339
    }
}

#[derive(foxglove::Encode)]
struct CsvRow {
    row_index: u64,
    values: Vec<KeyValuePair>,
}

impl UpstreamServer for CsvUpstream {
    type QueryParams = CsvParams;
    type Error = BoxError;

    async fn auth(&self, _bearer_token: Option<&str>, _params: &CsvParams) -> Result<(), AuthError> {
        // No authentication required for this demo
        Ok(())
    }

    async fn build_source(
        &self,
        params: CsvParams,
        mut source: SourceBuilder<'_>,
    ) -> Result<(), BoxError> {
        let topic = normalize_topic(params.topic.as_deref());
        let channel = source.channel::<CsvRow>(topic);

        if let Some(opts) = source.manifest() {
            let (start_time, end_time) = scan_time_range(&params)?;
            *opts = ManifestOpts {
                id: generate_source_id("csv", 1, &params),
                name: format!("CSV {}", params.csv_path),
                start_time,
                end_time,
            };
        }

        let Some(mut handle) = source.into_stream_handle() else {
            return Ok(());
        };

        tracing::info!(csv_path = %params.csv_path, "streaming csv data");
        stream_csv(&params, &channel, &mut handle).await?;
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
    serve(CsvUpstream, bind_address).await
}

async fn stream_csv(
    params: &CsvParams,
    channel: &MaybeChannel<CsvRow>,
    handle: &mut StreamHandle,
) -> Result<(), BoxError> {
    let mut reader = open_csv_reader(&params.csv_path)?;
    let headers = read_headers(&mut reader)?;
    let timestamp_index = find_timestamp_column(&headers, &params.timestamp_column)?;

    for (row_index, record) in reader.records().enumerate() {
        let record = record?;
        let timestamp = parse_timestamp_from_record(&record, timestamp_index, params.timestamp_format)?;
        let values = headers
            .iter()
            .enumerate()
            .map(|(index, name)| KeyValuePair {
                key: name.clone(),
                value: record.get(index).unwrap_or_default().to_string(),
            })
            .collect();

        channel.log_with_time(
            &CsvRow {
                row_index: row_index as u64,
                values,
            },
            timestamp,
        );

        if handle.buffer_size() >= MAX_BUFFER_SIZE {
            handle.flush().await?;
        }
    }

    Ok(())
}

fn scan_time_range(params: &CsvParams) -> Result<(DateTime<Utc>, DateTime<Utc>), BoxError> {
    let mut reader = open_csv_reader(&params.csv_path)?;
    let headers = read_headers(&mut reader)?;
    let timestamp_index = find_timestamp_column(&headers, &params.timestamp_column)?;

    let mut start_time: Option<DateTime<Utc>> = None;
    let mut end_time: Option<DateTime<Utc>> = None;

    for record in reader.records() {
        let record = record?;
        let timestamp = parse_timestamp_from_record(&record, timestamp_index, params.timestamp_format)?;
        start_time = Some(match start_time {
            Some(current) if timestamp < current => timestamp,
            Some(current) => current,
            None => timestamp,
        });
        end_time = Some(match end_time {
            Some(current) if timestamp > current => timestamp,
            Some(current) => current,
            None => timestamp,
        });
    }

    let Some(start_time) = start_time else {
        return Err(invalid_input("CSV file contains no data rows"));
    };
    let end_time = end_time.expect("end_time must be set when start_time is set");
    Ok((start_time, end_time))
}

fn open_csv_reader(path: &str) -> Result<csv::Reader<std::fs::File>, BoxError> {
    let file = std::fs::File::open(path)?;
    Ok(csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(file))
}

fn read_headers(reader: &mut csv::Reader<std::fs::File>) -> Result<Vec<String>, BoxError> {
    let headers = reader.headers()?.clone();
    Ok(headers.iter().map(|header| header.to_string()).collect())
}

fn find_timestamp_column(headers: &[String], column: &str) -> Result<usize, BoxError> {
    headers.iter().position(|header| header == column).ok_or_else(|| {
        invalid_input(format!(
            "timestamp column '{column}' not found (headers: {})",
            headers.join(", ")
        ))
    })
}

fn parse_timestamp_from_record(
    record: &csv::StringRecord,
    index: usize,
    format: TimestampFormat,
) -> Result<DateTime<Utc>, BoxError> {
    let value = record
        .get(index)
        .ok_or_else(|| invalid_input("timestamp column missing in row"))?;
    parse_timestamp(value, format)
}

fn parse_timestamp(value: &str, format: TimestampFormat) -> Result<DateTime<Utc>, BoxError> {
    match format {
        TimestampFormat::Rfc3339 => Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc)),
        TimestampFormat::UnixSeconds => parse_unix_timestamp(value, 1_000_000_000),
        TimestampFormat::UnixMillis => parse_unix_timestamp(value, 1_000_000),
        TimestampFormat::UnixMicros => parse_unix_timestamp(value, 1_000),
        TimestampFormat::UnixNanos => parse_unix_timestamp(value, 1),
    }
}

fn parse_unix_timestamp(value: &str, scale: i128) -> Result<DateTime<Utc>, BoxError> {
    let raw: i128 = value.parse().map_err(|error| {
        invalid_input(format!("invalid unix timestamp '{value}': {error}"))
    })?;
    let nanos = raw
        .checked_mul(scale)
        .ok_or_else(|| invalid_input("timestamp out of range"))?;
    let secs = nanos.div_euclid(1_000_000_000);
    let sub_nanos = nanos.rem_euclid(1_000_000_000);
    let secs = i64::try_from(secs).map_err(|_| invalid_input("timestamp out of range"))?;
    let sub_nanos = u32::try_from(sub_nanos).map_err(|_| invalid_input("timestamp out of range"))?;
    Utc.timestamp_opt(secs, sub_nanos)
        .single()
        .ok_or_else(|| invalid_input("timestamp out of range"))
}

fn normalize_topic(topic: Option<&str>) -> String {
    let trimmed = topic.unwrap_or("/csv").trim();
    if trimmed.is_empty() {
        "/csv".to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn default_timestamp_column() -> String {
    "timestamp".to_string()
}

fn invalid_input(message: impl Into<String>) -> BoxError {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into()).into()
}
