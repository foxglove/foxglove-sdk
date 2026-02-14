//! Example using the Foxglove WebSocket server with the PlaybackControl capability.
//!
//! This example plays back simple dummy data points from an in-memory buffer, generated at 10Hz between epoch
//! timestamps 0 and 10s.
//!
//! In a real implementation you would replace the in-memory buffer and playback logic with your own data source
//! (e.g. reading from files or a database), implement your own time-tracking logic, and write
//! a custom `ServerListener` to handle `PlaybackControlRequest`s appropriate to your use case.

use std::sync::{Arc, Mutex};

use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus,
    ServerListener,
};
use foxglove::{ChannelBuilder, PartialMetadata};

struct DataPoint {
    timestamp: u64,
    value: f64,
}

struct PlayerState {
    status: PlaybackStatus,
    current_message_index: usize,
    playback_speed: f32,
}

// Simple `ServerListener` that tracks variables required for playback and handles playback control
// requests from Foxglove.
struct DataPointPlayer {
    state: Arc<Mutex<PlayerState>>,
    data_len: usize,
}

impl DataPointPlayer {
    fn seek(&self, seek_time: u64) -> usize {
        let index = (seek_time / 100_000_000) as usize;
        index.min(self.data_len.saturating_sub(1))
    }
}

impl ServerListener for DataPointPlayer {
    // Handle a playback control request from Foxglove and return an updated playback state. In
    // your application, you'd implement this function to handle the request as appropriate for
    // your data source and playback logic.
    fn on_playback_control_request(
        &self,
        request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        tracing::info!("Received playback control request: {request:?}");

        let mut state = self.state.lock().unwrap();

        // Handle playback command
        match request.playback_command {
            PlaybackCommand::Play => {
                state.status = PlaybackStatus::Playing;
            }
            PlaybackCommand::Pause => {
                state.status = PlaybackStatus::Paused;
            }
        }

        // Handle playback speed request, clamping to a reasonable lower bound
        state.playback_speed = f32::max(0.001, request.playback_speed);

        // Handle seeking
        let did_seek = request.seek_time.is_some();
        if let Some(seek_time) = request.seek_time {
            state.current_message_index = self.seek(seek_time);
        }

        // Clamp the index to the valid range to ensure current_time stays within playback_time_range
        let clamped_index = state.current_message_index.min(self.data_len.saturating_sub(1));
        let current_time = clamped_index as u64 * 100_000_000;

        // Return the updated playback state
        let response = PlaybackState {
            status: state.status,
            current_time,
            playback_speed: state.playback_speed,
            did_seek,
            request_id: Some(request.request_id),
        };
        tracing::info!("Sending playback state: {response:?}");
        Some(response)
    }
}

async fn playback_loop(
    data: &[DataPoint],
    state: &Mutex<PlayerState>,
    channel: &foxglove::RawChannel,
    server: &foxglove::WebSocketServerHandle,
) {
    loop {
        let (status, current_message_index, speed) = {
            let s = state.lock().unwrap();
            (s.status, s.current_message_index, s.playback_speed)
        };

        match status {
            PlaybackStatus::Playing => {
                // Out of data; broadcast a PlaybackState indicating that playback has ended
                if current_message_index >= data.len() {
                    {
                        let mut s = state.lock().unwrap();
                        s.status = PlaybackStatus::Ended;
                    }
                    server.broadcast_playback_state(PlaybackState {
                        status: PlaybackStatus::Ended,
                        current_time: data.last().map_or(0, |d| d.timestamp),
                        playback_speed: speed,
                        did_seek: false,
                        request_id: None,
                    });
                    continue;
                }

                let point = &data[current_message_index];

                // Broadcast the current log time...
                server.broadcast_time(point.timestamp);

                //... then log the data over the WebSocket
                let payload = format!(r#"{{"value": {:.1}}}"#, point.value);
                channel.log_with_meta(
                    payload.as_bytes(),
                    PartialMetadata::with_log_time(point.timestamp),
                );

                {
                    let mut s = state.lock().unwrap();
                    s.current_message_index += 1;
                }

                // Sleep, accounting for the current playback speed
                let sleep_ms = (100.0 / speed as f64).max(1.0) as u64;
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }
            _ => {
                // If not actively playing, sleep
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    }
}

// Generate dummy data for playback
// In this example, we use a simple linear function ranging from t=0 to t=10s, generated at 10Hz
fn generate_data() -> Vec<DataPoint> {
    (0..=100)
        .map(|i| {
            let timestamp = i as u64 * 100_000_000; // 10Hz = 100ms intervals
            let value = i as f64 * 0.1;
            DataPoint { timestamp, value }
        })
        .collect()
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let data = generate_data();

    let channel = ChannelBuilder::new("/data")
        .message_encoding("json")
        .build_raw()
        .expect("Failed to create channel");

    let state = Arc::new(Mutex::new(PlayerState {
        status: PlaybackStatus::Paused,
        current_message_index: 0,
        playback_speed: 1.0,
    }));

    let listener = Arc::new(DataPointPlayer {
        state: Arc::clone(&state),
        data_len: data.len(),
    });

    // Set up the server
    //
    // To implement the `PlaybackControl` capability, we:
    // - advertise the `PlaybackControl` and `Time` capabilities
    // - declare the playback time range in nanoseconds since epoch
    // - register our `ServerListener` for handling `PlaybackControlRequest`s
    let server = foxglove::WebSocketServer::new()
        .name("ws_playback_control")
        .bind("127.0.0.1", 8765)
        .capabilities([Capability::PlaybackControl, Capability::Time])
        .playback_time_range(0, 10_000_000_000)
        .listener(listener)
        .start()
        .await
        .expect("Server failed to start");

    tracing::info!("View in browser: {}", server.app_url());

    tokio::select! {
        _ = playback_loop(&data, &state, &channel, &server) => {}
        _ = tokio::signal::ctrl_c() => {}
    }

    server.stop().wait().await;
}
