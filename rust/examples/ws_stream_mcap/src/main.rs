//! Streams an mcap file over a websocket.

mod mcap_player;
mod playback_source;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcap_player::McapPlayer;
use playback_source::PlaybackSource;

use anyhow::Result;
use clap::Parser;
use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, ServerListener,
};
use foxglove::WebSocketServer;
use tracing::info;

struct Listener {
    player: Arc<Mutex<dyn Send + Sync + PlaybackSource>>,
}

impl Listener {
    fn new(player: Arc<Mutex<dyn Send + Sync + PlaybackSource>>) -> Self {
        Self { player }
    }
}

impl ServerListener for Listener {
    fn on_playback_control_request(
        &self,
        request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        let mut player = self.player.lock().unwrap();

        if let Some(seek_time) = request.seek_time {
            player.seek(seek_time).ok()?
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
            did_seek: request.seek_time.is_some(),
            request_id: Some(request.request_id),
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
    /// Whether to loop.
    #[arg(long)]
    r#loop: bool,
}

fn main() -> Result<()> {
    let env = env_logger::Env::default().default_filter_or("debug");
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

    info!("Loading mcap summary");
    let mcap_player = Arc::new(Mutex::new(McapPlayer::new(args.path)));
    let listener = Listener::new(mcap_player);

    let server = WebSocketServer::new()
        .name(file_name)
        .capabilities([Capability::Time])
        .bind(&args.host, args.port)
        .start_blocking()
        .expect("Server failed to start");

    info!("Waiting for client");
    std::thread::sleep(Duration::from_secs(1));

    info!("Starting stream");
    while !done.load(Ordering::Relaxed) {
        summary.mcap_player().stream_until(&server, &done)?;
        if !args.r#loop {
            done.store(true, Ordering::Relaxed);
        } else {
            info!("Looping");
            server.clear_session(None);
        }
    }

    server.stop().wait_blocking();
    Ok(())
}
