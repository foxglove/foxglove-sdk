//! Streams an mcap file over a websocket.

mod mcap_player;
mod playback_source;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus,
    ServerListener,
};
use foxglove::WebSocketServer;
use tracing::{info, warn};

use mcap_player::McapPlayer;
use playback_source::PlaybackSource;

struct Listener {
    playback: Arc<Mutex<dyn PlaybackSource + Send>>,
}

impl Listener {
    fn new(playback: Arc<Mutex<dyn PlaybackSource + Send>>) -> Self {
        Self { playback }
    }

    fn current_state(&self) -> PlaybackState {
        let playback = self.playback.lock().unwrap();
        PlaybackState {
            status: playback.status(),
            current_time: playback.current_time(),
            playback_speed: playback.playback_speed(),
            did_seek: false,
            request_id: None,
        }
    }
}

impl ServerListener for Listener {
    fn on_playback_control_request(
        &self,
        request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        let mut playback = self.playback.lock().unwrap();
        let mut status = match request.playback_command {
            PlaybackCommand::Play => PlaybackStatus::Playing,
            PlaybackCommand::Pause => PlaybackStatus::Paused,
        };

        info!(
            "Handled requested playback command {:?}",
            request.playback_command
        );

        let mut did_seek = false;
        if let Some(seek_time) = request.seek_time {
            info!("Requested seek to {}ns", seek_time);
            if let Err(err) = playback.seek(seek_time) {
                warn!("Failed to seek to {}ns: {err}", seek_time);
                status = PlaybackStatus::Paused;
            } else {
                did_seek = true;
            }
        }

        if (request.playback_speed - playback.playback_speed()).abs() > f32::EPSILON {
            playback.set_playback_speed(request.playback_speed);
        }

        match status {
            PlaybackStatus::Playing => playback.play(),
            PlaybackStatus::Paused | PlaybackStatus::Buffering | PlaybackStatus::Ended => {
                playback.pause()
            }
        }

        Some(PlaybackState {
            status: playback.status(),
            current_time: playback.current_time(),
            playback_speed: playback.playback_speed(),
            request_id: Some(request.request_id),
            did_seek,
        })
    }
}

#[derive(Debug, Parser)]
struct Cli {
    /// Server TCP port.
    #[arg(short, long, default_value_t = 8765)]
    port: u16,
    /// Server IP address.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// MCAP file to read.
    #[arg(short, long)]
    file: PathBuf,
}

fn main() -> Result<()> {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Cli::parse();
    let file_name = args
        .file
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();

    let done = Arc::new(AtomicBool::default());
    ctrlc::set_handler({
        let done = done.clone();
        move || {
            done.store(true, Ordering::Relaxed);
        }
    })
    .expect("Failed to set SIGINT handler");

    let player: Arc<Mutex<McapPlayer>> = Arc::new(Mutex::new(McapPlayer::new(&args.file)?));
    let (start_time, end_time) = player.lock().unwrap().time_bounds();

    info!(
        "Found time bounds in mcap file: ({}, {})",
        start_time, end_time
    );

    let listener = Arc::new(Listener::new(player.clone()));

    let server = WebSocketServer::new()
        .name(file_name)
        .capabilities([Capability::Time])
        .listener(listener.clone())
        .playback_time_range(start_time, end_time)
        .bind(&args.host, args.port)
        .start_blocking()
        .expect("Server failed to start");

    info!("Waiting for client");
    std::thread::sleep(Duration::from_secs(1));

    info!("Starting stream");
    while !done.load(Ordering::Relaxed) {
        let state = listener.current_state();
        if state.status != PlaybackStatus::Playing {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        let next_wakeup = { player.lock().unwrap().next_wakeup()? };

        let Some(next_wakeup) = next_wakeup else {
            let ended = { player.lock().unwrap().status() == PlaybackStatus::Ended };
            if ended {
                info!("Playback complete");
                server.broadcast_playback_state(listener.current_state());
            }
            continue;
        };

        let now = Instant::now();
        if next_wakeup > now {
            std::thread::sleep(next_wakeup - now);
        }

        {
            let mut player = player.lock().unwrap();
            if let Some(timestamp) = player.should_broadcast_time() {
                server.broadcast_time(timestamp);
            }

            player.flush_since_last()?;
        }
    }

    server.stop().wait_blocking();
    Ok(())
}
