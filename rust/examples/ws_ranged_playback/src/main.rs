//! Minimal ranged playback example.
//!
//! Serves 10 seconds of synthetic data at 10Hz on a `/data` channel,
//! controlled by Foxglove's playback bar via the RangedPlayback capability.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus,
    ServerListener,
};
use foxglove::{ChannelBuilder, PartialMetadata, RawChannel, WebSocketServer, WebSocketServerHandle};
use tracing::info;

/// Number of messages in the dataset.
const NUM_MESSAGES: usize = 100;
/// Interval between messages in nanoseconds (100ms = 10Hz).
const INTERVAL_NS: u64 = 100_000_000;
/// Start timestamp in nanoseconds.
const START_TIME_NS: u64 = 0;
/// End timestamp in nanoseconds (inclusive).
const END_TIME_NS: u64 = START_TIME_NS + (NUM_MESSAGES as u64 - 1) * INTERVAL_NS;
/// Minimum playback speed.
const MIN_PLAYBACK_SPEED: f32 = 0.01;

/// Returns the log timestamp for a given message index.
fn timestamp_for_index(index: usize) -> u64 {
    START_TIME_NS + (index as u64) * INTERVAL_NS
}

/// Returns the message index for a given log timestamp.
fn index_for_timestamp(timestamp: u64) -> usize {
    let offset = timestamp.saturating_sub(START_TIME_NS);
    let index = (offset / INTERVAL_NS) as usize;
    index.min(NUM_MESSAGES - 1)
}

fn clamp_speed(speed: f32) -> f32 {
    if speed.is_finite() && speed >= MIN_PLAYBACK_SPEED {
        speed
    } else {
        MIN_PLAYBACK_SPEED
    }
}

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

struct Player {
    channel: Arc<RawChannel>,
    status: PlaybackStatus,
    current_index: usize,
    current_time: u64,
    playback_speed: f32,
    /// Wall-clock deadline for the next message emission.
    /// `None` means the current message should be emitted immediately.
    next_emit_time: Option<Instant>,
}

impl Player {
    fn new(channel: Arc<RawChannel>) -> Self {
        Self {
            channel,
            status: PlaybackStatus::Paused,
            current_index: 0,
            current_time: START_TIME_NS,
            playback_speed: 1.0,
            next_emit_time: None,
        }
    }

    fn status(&self) -> PlaybackStatus {
        self.status
    }

    fn current_time(&self) -> u64 {
        self.current_time
    }

    fn playback_speed(&self) -> f32 {
        self.playback_speed
    }

    fn play(&mut self) {
        // Don't transition to Playing if playback has ended.
        // To restart, the caller must seek first.
        if self.status == PlaybackStatus::Ended {
            return;
        }
        self.status = PlaybackStatus::Playing;
    }

    fn pause(&mut self) {
        self.status = PlaybackStatus::Paused;
        self.next_emit_time = None;
    }

    fn seek(&mut self, log_time: u64) {
        let log_time = log_time.clamp(START_TIME_NS, END_TIME_NS);
        self.current_index = index_for_timestamp(log_time);
        self.current_time = timestamp_for_index(self.current_index);
        self.next_emit_time = None;
        if self.status == PlaybackStatus::Ended {
            self.status = PlaybackStatus::Paused;
        }
    }

    fn set_playback_speed(&mut self, speed: f32) {
        self.playback_speed = clamp_speed(speed);
    }

    /// Logs the next message if it is ready, or returns a duration to sleep.
    ///
    /// Returns `Some(duration)` if the caller should sleep before calling again,
    /// or `None` if a message was logged (or playback is not active).
    fn log_next_message(&mut self, server: &WebSocketServerHandle) -> Option<Duration> {
        if self.status != PlaybackStatus::Playing {
            return None;
        }

        if self.current_index >= NUM_MESSAGES {
            self.status = PlaybackStatus::Ended;
            self.current_time = END_TIME_NS;
            return None;
        }

        // Wait until it's time to emit the next message.
        if let Some(deadline) = self.next_emit_time {
            if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                return Some(remaining);
            }
        }

        // Broadcast time before the data message. Both go through the data plane,
        // so FIFO ordering is preserved.
        let msg_time = timestamp_for_index(self.current_index);
        self.current_time = msg_time;
        server.broadcast_time(msg_time);

        let value = self.current_index;
        let json = format!(r#"{{"value": {value}}}"#);
        self.channel.log_with_meta(
            json.as_bytes(),
            PartialMetadata {
                log_time: Some(msg_time),
            },
        );

        self.current_index += 1;
        let interval = Duration::from_secs_f64(0.1 / self.playback_speed as f64);
        self.next_emit_time = Some(Instant::now() + interval);
        None
    }
}

// ---------------------------------------------------------------------------
// Listener
// ---------------------------------------------------------------------------

struct Listener {
    player: Arc<Mutex<Player>>,
}

impl ServerListener for Listener {
    fn on_playback_control_request(
        &self,
        request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        let mut player = self.player.lock().unwrap();

        // Handle seek first, before play/pause. This is important for looping,
        // where Foxglove sends a seek to the beginning followed by a Play command.
        let did_seek = request.seek_time.is_some();
        if let Some(seek_time) = request.seek_time {
            player.seek(seek_time);
        }

        player.set_playback_speed(request.playback_speed);

        match request.playback_command {
            PlaybackCommand::Play => player.play(),
            PlaybackCommand::Pause => player.pause(),
        };

        Some(PlaybackState {
            current_time: player.current_time(),
            playback_speed: player.playback_speed(),
            status: player.status(),
            did_seek,
            request_id: Some(request.request_id),
        })
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let done = Arc::new(AtomicBool::default());
    ctrlc::set_handler({
        let done = done.clone();
        move || {
            done.store(true, Ordering::Relaxed);
        }
    })
    .expect("Failed to set SIGINT handler");

    let channel = ChannelBuilder::new("/data")
        .message_encoding("json")
        .build_raw()
        .expect("Failed to create channel");

    let player = Arc::new(Mutex::new(Player::new(channel)));
    let listener = Arc::new(Listener {
        player: player.clone(),
    });

    let server = WebSocketServer::new()
        .name("ranged-playback-example")
        .capabilities([Capability::RangedPlayback, Capability::Time])
        .playback_time_range(START_TIME_NS, END_TIME_NS)
        .listener(listener)
        .start_blocking()
        .expect("Server failed to start");

    info!("Server started, waiting for client");

    let mut last_status = PlaybackStatus::Paused;
    while !done.load(Ordering::Relaxed) {
        let status = {
            let p = player.lock().unwrap();
            let status = p.status();

            // Broadcast state change when playback ends
            if status == PlaybackStatus::Ended && last_status != PlaybackStatus::Ended {
                server.broadcast_playback_state(PlaybackState {
                    current_time: p.current_time(),
                    playback_speed: p.playback_speed(),
                    status,
                    did_seek: false,
                    request_id: None,
                });
            }

            status
        };
        last_status = status;

        if status != PlaybackStatus::Playing {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        // Log next message, sleeping outside the lock if needed
        let sleep_duration = player.lock().unwrap().log_next_message(&server);
        if let Some(duration) = sleep_duration {
            std::thread::sleep(std::cmp::min(duration, Duration::from_secs(1)));
        }
    }

    server.stop().wait_blocking();
}
