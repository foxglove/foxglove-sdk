//! Example: Playing back CSV data with RangedPlayback capability
//!
//! This example demonstrates how to implement the RangedPlayback capability to play back
//! time-series data from a CSV file. It shows how to:
//!
//! 1. Declare the time range of data using `playback_time_range()`
//! 2. Implement `on_playback_control_request()` in a ServerListener
//! 3. Call `broadcast_time()` after logging each message (Time capability)
//! 4. Call `broadcast_playback_state()` when playback state changes
//!
//! The CSV file should have two columns: `timestamp` (nanoseconds) and `data` (float).
//!
//! Run with: cargo run -p example_ws_stream_csv -- path/to/data.csv

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus,
    ServerListener,
};
use foxglove::{
    ChannelBuilder, PartialMetadata, RawChannel, Schema, WebSocketServer, WebSocketServerHandle,
};

/// Play back CSV data over a WebSocket with RangedPlayback support.
#[derive(Debug, Parser)]
struct Cli {
    /// CSV file to play back.
    /// Expected format: timestamp,data (timestamp in nanoseconds, data as float)
    file: PathBuf,
}

/// A single data point from the CSV file.
#[derive(Debug, Clone)]
struct DataPoint {
    timestamp: u64, // nanoseconds
    value: f64,
}

/// Load CSV data from file.
/// Expected format: timestamp,data (with header row)
fn load_csv(path: &PathBuf) -> Result<Vec<DataPoint>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut csv_reader = csv::Reader::from_reader(reader);

    let mut data = Vec::new();
    for result in csv_reader.records() {
        let record = result?;
        let timestamp: u64 = record[0].parse()?;
        let value: f64 = record[1].parse()?;
        data.push(DataPoint { timestamp, value });
    }

    // Sort by timestamp to ensure correct playback order
    data.sort_by_key(|d| d.timestamp);
    Ok(data)
}

/// Simple controller for managing the playback of data. This is an example stand-in
/// for your own custom data playback stack.
struct PlaybackController {
    inner: Inner,
}

/// Internal variables owned by PlaybackController required for tracking the current
/// playback time, status, speed, etc. These will be copied over to an emitted PlaybackState message.
#[derive(Clone, Copy)]
struct Inner {
    status: PlaybackStatus,
    speed: f32,
    current_time: u64,
    did_seek: bool,
    time_range: (u64, u64),
}

impl PlaybackController {
    fn new(time_range: (u64, u64)) -> Self {
        Self {
            inner: Inner {
                status: PlaybackStatus::Paused,
                current_time: time_range.0,
                speed: 1.0,
                did_seek: false,
                time_range,
            },
        }
    }

    /// Create a PlaybackState message from the current state.
    fn to_playback_state(&self, request_id: Option<String>) -> PlaybackState {
        PlaybackState {
            status: self.inner.status,
            current_time: self.inner.current_time,
            playback_speed: self.inner.speed,
            did_seek: self.inner.did_seek,
            request_id,
        }
    }
}

/// ServerListener implementation that handles playback control requests from Foxglove.
struct CsvPlayerListener {
    controller: Arc<Mutex<PlaybackController>>,
}

impl ServerListener for CsvPlayerListener {
    /// Handle playback control requests from Foxglove.
    ///
    /// This is called when the user interacts with the playback bar in Foxglove.
    /// We must return a PlaybackState reflecting the new state after handling the request.
    fn on_playback_control_request(
        &self,
        request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        let mut controller = self.controller.lock().unwrap();

        // Handle play/pause command
        match request.playback_command {
            PlaybackCommand::Play => {
                controller.inner.status = PlaybackStatus::Playing;
            }
            PlaybackCommand::Pause => {
                controller.inner.status = PlaybackStatus::Paused;
            }
        }

        // Update playback speed
        controller.inner.speed = request.playback_speed;

        // Handle seek if requested
        controller.inner.did_seek = false;
        if let Some(seek_time) = request.seek_time {
            // Clamp seek time to valid range
            controller.inner.current_time =
                seek_time.clamp(controller.inner.time_range.0, controller.inner.time_range.1);
            controller.inner.did_seek = true;
        }

        // Return the new playback state with the request_id for correlation
        Some(controller.to_playback_state(Some(request.request_id)))
    }
}

/// Main playback loop that publishes data and time updates.
fn handle_seek(
    data: &[DataPoint],
    current_time: u64,
    current_index: &mut usize,
    last_wall_time: &mut Instant,
    last_playback_time: &mut Option<u64>,
) {
    *current_index = data
        .iter()
        .position(|d| d.timestamp >= current_time)
        .unwrap_or(data.len());
    *last_wall_time = Instant::now();
    *last_playback_time = Some(current_time);
}

fn publish_until(
    server: &WebSocketServerHandle,
    channel: &RawChannel,
    data: &[DataPoint],
    current_index: &mut usize,
    new_playback_time: u64,
) {
    while *current_index < data.len() && data[*current_index].timestamp <= new_playback_time {
        let point = &data[*current_index];

        // Create JSON payload matching our simple schema
        let json_payload = format!(
            r#"{{"timestamp":{},"value":{}}}"#,
            point.timestamp, point.value
        );

        // Log the data to the channel with the data's timestamp
        channel.log_with_meta(
            json_payload.as_bytes(),
            PartialMetadata::with_log_time(point.timestamp),
        );

        // IMPORTANT: Broadcast time after each message so Foxglove knows the current time
        server.broadcast_time(point.timestamp);

        *current_index += 1;
    }
}

fn update_current_time(
    controller: &Arc<Mutex<PlaybackController>>,
    new_playback_time: u64,
    end_time: u64,
) {
    let mut ctrl = controller.lock().unwrap();
    ctrl.inner.current_time = new_playback_time.min(end_time);
}

fn handle_end_state(
    server: &WebSocketServerHandle,
    controller: &Arc<Mutex<PlaybackController>>,
    end_time: u64,
) {
    let mut ctrl = controller.lock().unwrap();
    ctrl.inner.status = PlaybackStatus::Ended;
    ctrl.inner.current_time = end_time;

    // IMPORTANT: Broadcast PlaybackState after ending so Foxglove can stay in sync
    server.broadcast_playback_state(ctrl.to_playback_state(None));
    println!("Playback ended. Waiting for seek or restart...");

    // Wait for user to seek or restart
    drop(ctrl);
    loop {
        thread::sleep(Duration::from_millis(100));
        let ctrl = controller.lock().unwrap();
        if ctrl.inner.did_seek || ctrl.inner.status == PlaybackStatus::Playing {
            break;
        }
    }
}

fn run_playback_loop(
    server: &WebSocketServerHandle,
    channel: &RawChannel,
    data: &[DataPoint],
    controller: Arc<Mutex<PlaybackController>>,
) {
    let mut current_index = 0;
    let mut last_wall_time = Instant::now();
    let mut last_playback_time: Option<u64> = None;

    loop {
        thread::sleep(Duration::from_millis(10)); // Small sleep to prevent busy-waiting

        let inner = {
            let mut ctrl = controller.lock().unwrap();
            let inner = ctrl.inner;
            // Clear the did_seek flag after reading
            ctrl.inner.did_seek = false;
            inner
        };

        // If we seeked, find the new playback position in the data
        if inner.did_seek || last_playback_time.is_none() {
            handle_seek(
                data,
                inner.current_time,
                &mut current_index,
                &mut last_wall_time,
                &mut last_playback_time,
            );
        }

        // Only advance time if playing
        if inner.status != PlaybackStatus::Playing {
            last_wall_time = Instant::now();
            continue;
        }

        // Calculate how much playback time has elapsed based on wall time and speed
        let wall_elapsed = last_wall_time.elapsed();
        let playback_elapsed_ns = (wall_elapsed.as_nanos() as f64 * inner.speed as f64) as u64;
        let new_playback_time =
            last_playback_time.unwrap_or(inner.current_time) + playback_elapsed_ns;

        // Publish all data points up to the current playback time
        publish_until(server, channel, data, &mut current_index, new_playback_time);

        // Update controller with new current time
        update_current_time(&controller, new_playback_time, inner.time_range.1);

        // Reset timing references for next iteration
        last_wall_time = Instant::now();
        last_playback_time = Some(new_playback_time);

        // Check if we've reached the end of the data
        if new_playback_time >= inner.time_range.1 {
            handle_end_state(server, &controller, inner.time_range.1);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    // Load the CSV data
    println!("Loading CSV from: {}", args.file.display());
    let data = load_csv(&args.file)?;

    if data.is_empty() {
        eprintln!("Error: CSV file is empty or has no valid data");
        std::process::exit(1);
    }

    // Determine time range from the data
    let start_time = data.first().unwrap().timestamp;
    let end_time = data.last().unwrap().timestamp;
    println!(
        "Data time range: {} - {} ({} points)",
        start_time,
        end_time,
        data.len()
    );

    // Create shared playback controller
    let controller = Arc::new(Mutex::new(PlaybackController::new((start_time, end_time))));

    // Create the server listener
    let listener = Arc::new(CsvPlayerListener {
        controller: controller.clone(),
    });

    // Create the channel for publishing CSV data
    // Using JSON encoding with a simple schema
    let channel = ChannelBuilder::new("/csv_data")
        .message_encoding("json")
        .schema(Schema::new(
            "CsvDataPoint",
            "jsonschema",
            r#"{"type":"object","properties":{"timestamp":{"type":"integer"},"value":{"type":"number"}}}"#.as_bytes(),
        ))
        .build_raw()
        .expect("Failed to create channel");

    // Create and start the WebSocket server with required capabilities:
    // 1. Time capability: required to broadcast current time with broadcast_time()
    // 2. RangedPlayback: automatically added when playback_time_range() is called
    let server = WebSocketServer::new()
        .name("CSV Player Example")
        .capabilities([Capability::Time]) // Time capability is required for RangedPlayback
        .playback_time_range(start_time, end_time) // Declares data range; adds RangedPlayback capability
        .listener(listener)
        .start_blocking()?;

    println!("Server started on port {}", server.port());
    println!("Open Foxglove and connect to: {}", server.app_url());

    // Run the playback loop (blocks forever)
    run_playback_loop(&server, &channel, &data, controller);

    Ok(())
}
