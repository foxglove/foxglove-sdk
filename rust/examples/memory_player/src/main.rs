use foxglove::LazyChannel;
use rand::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use foxglove::websocket::{PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus};

#[derive(foxglove::Encode, Clone)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(foxglove::Encode)]
struct IMUMessage {
    timestamp: u64,
    linear_velocity: Vec3,
}
static IMU_CHANNEL: LazyChannel<IMUMessage> = LazyChannel::new("/imu");

static DT: f64 = 1.0; // seconds
static NUM_FRAMES: u64 = 100;

fn initialize_imu_messages() -> Vec<IMUMessage> {
    let mut rng: StdRng = rand::SeedableRng::seed_from_u64(2);
    let mut imu_messages = Vec::with_capacity(NUM_FRAMES as usize);

    for frame in 0..NUM_FRAMES {
        let current_time_nanos = frame * ((DT * 1_000_000_000.0) as u64);
        imu_messages.push(IMUMessage {
            timestamp: current_time_nanos,
            linear_velocity: Vec3 {
                x: rng.random_range(-10.0..10.0),
                y: rng.random_range(-10.0..10.0),
                z: rng.random_range(-10.0..10.0),
            },
        });
    }

    imu_messages
}
fn playback_range(imu_messages: &[IMUMessage]) -> (u64, u64) {
    if imu_messages.is_empty() {
        return (0, 0);
    }
    (
        imu_messages[0].timestamp,
        imu_messages[imu_messages.len() - 1].timestamp,
    )
}

struct PlayerState {
    imu_messages: Arc<Vec<IMUMessage>>,
    playback_index: usize,
    playback_time: u64,
    playback_state: PlaybackState,
}

impl PlayerState {
    fn new(imu_messages: Arc<Vec<IMUMessage>>) -> Self {
        let initial_time = imu_messages.first().map(|msg| msg.timestamp).unwrap_or(0);
        let playback_state = PlaybackState {
            status: PlaybackStatus::Paused,
            playback_speed: 1.0,
            current_time: initial_time,
            request_id: None,
        };

        Self {
            imu_messages,
            playback_index: 0,
            playback_time: initial_time,
            playback_state,
        }
    }

    fn handle_seek(&mut self, seek_time_ns: u64) {
        if self.imu_messages.is_empty() {
            self.playback_index = 0;
            self.playback_time = 0;
            self.playback_state.current_time = 0;
            return;
        }

        // Binary search to find the first message at or before the seek time
        self.playback_index = match self
            .imu_messages
            .binary_search_by_key(&seek_time_ns, |msg| msg.timestamp)
        {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        self.playback_time = seek_time_ns;
        self.playback_state.current_time = seek_time_ns;
    }
}

type SharedPlayerState = Arc<Mutex<PlayerState>>;

struct Player {
    state: SharedPlayerState,
}

impl Player {
    fn new(state: SharedPlayerState) -> Self {
        Self { state }
    }

    async fn play(&mut self) {
        loop {
            let (
                status,
                playback_speed,
                playback_index,
                playback_time,
                imu_messages_len,
                imu_messages,
            ) = {
                let mut state = self.state.lock().unwrap();
                let imu_messages = Arc::clone(&state.imu_messages);
                let imu_messages_len = imu_messages.len();

                if imu_messages_len == 0 {
                    state.playback_index = 0;
                    state.playback_time = 0;
                    state.playback_state.current_time = 0;
                } else if state.playback_index >= imu_messages_len {
                    state.playback_index = 0;
                    state.playback_time = imu_messages[0].timestamp;
                    state.playback_state.current_time = state.playback_time;
                }

                (
                    state.playback_state.status,
                    state.playback_state.playback_speed,
                    state.playback_index,
                    state.playback_time,
                    imu_messages_len,
                    imu_messages,
                )
            };

            if !matches!(status, PlaybackStatus::Playing) {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }

            if imu_messages_len == 0 || playback_index >= imu_messages_len {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }

            // Get messages between the playback_index and the playback_time
            let pub_iter = imu_messages
                .iter()
                .skip(playback_index)
                .take_while(|message| message.timestamp <= playback_time);

            let mut num_taken = 0;

            for message in pub_iter {
                IMU_CHANNEL.log(&self.transform_imu_message(message));
                num_taken += 1;
            }

            {
                let mut state = self.state.lock().unwrap();
                if state.playback_index == playback_index && state.playback_time == playback_time {
                    state.playback_index = playback_index
                        .saturating_add(num_taken)
                        .min(imu_messages_len);
                    state.playback_time = playback_time.saturating_add(1_000_000_000);
                    state.playback_state.current_time = state.playback_time;
                }
            }

            let playback_speed = if playback_speed <= 0.0 {
                1.0
            } else {
                playback_speed
            };

            tokio::time::sleep(Duration::from_nanos((1e9 / playback_speed as f64) as u64)).await;
        }
    }

    fn transform_imu_message(&self, message: &IMUMessage) -> IMUMessage {
        IMUMessage {
            timestamp: message.timestamp,
            linear_velocity: Vec3 {
                x: message.linear_velocity.x,
                y: message.linear_velocity.y,
                z: message.linear_velocity.z,
            },
        }
    }
}

struct Listener {
    state: SharedPlayerState,
}

impl Listener {
    fn new(state: SharedPlayerState) -> Self {
        Self { state }
    }
}

impl foxglove::websocket::ServerListener for Listener {
    fn on_playback_control_request(&self, request: PlaybackControlRequest) -> PlaybackState {
        let mut state = self.state.lock().unwrap();
        match request.playback_command {
            PlaybackCommand::Pause => state.playback_state.status = PlaybackStatus::Paused,
            PlaybackCommand::Play => state.playback_state.status = PlaybackStatus::Playing,
        }

        state.playback_state.playback_speed = request.playback_speed;

        if let Some(seek_time) = request.seek_time {
            state.handle_seek(seek_time);
        }

        state.playback_state.request_id = Some(request.request_id.clone());
        state.playback_state.clone()
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);
    let imu_messages = initialize_imu_messages();
    let playback_range = playback_range(&imu_messages);
    let imu_messages = Arc::new(imu_messages);
    let shared_state = Arc::new(Mutex::new(PlayerState::new(imu_messages)));

    let mut player = Player::new(shared_state.clone());
    let listener = Arc::new(Listener::new(shared_state.clone()));

    let server = foxglove::WebSocketServer::new()
        .bind("127.0.0.1", 8765)
        .playback_time_range(playback_range.0, playback_range.1)
        .listener(listener)
        .start()
        .await
        .expect("Failed to start websocket server");

    // Spawn the player task
    tokio::spawn(async move {
        player.play().await;
    });

    // Keep main running (wait for ctrl+c)
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl+c");
    server.stop().wait().await;
}
