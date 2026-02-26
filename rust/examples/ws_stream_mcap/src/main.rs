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
use foxglove::WebSocketServer;
use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus,
    ServerListener,
};
use tracing::info;

struct Listener {
    player: Arc<Mutex<dyn Send + PlaybackSource>>,
}

impl Listener {
    fn new(player: Arc<Mutex<dyn Send + PlaybackSource>>) -> Self {
        Self { player }
    }
}

/// Implement PlaybackControl-specific listener logic for responding to PlaybackControlRequests
impl ServerListener for Listener {
    /// Respond to a PlaybackControlRequest from Foxglove and send an updated PlaybackState.
    /// First we process the fields in the request (seeking, updating the playback speed, and
    /// handling play/pause PlaybackCommands by calling functions on our MCAP-specific PlaybackSource.
    /// Then we query the PlaybackSource to fill out the PlaybackState message sent in response to
    /// update Foxglove's UI.
    ///
    /// The intent of PlaybackSource is to let you implement the trait with your own data
    /// format, then reuse the structure of this function in your own player application.
    fn on_playback_control_request(
        &self,
        request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        let mut player = self.player.lock().unwrap();

        // Handle seek first, before play/pause. This is important for looping behavior,
        // where Foxglove sends a seek to the beginning followed by a Play command.
        // Setting this flag to true clears panels in the Foxglove player. For simplicity, we set
        // this every time a seek is requested from Foxglove. In your application, consider
        // implementing logic that determines whether a seek represents a significant jump in time
        // for the data you're playing back.
        let mut did_seek = request.seek_time.is_some();

        if let Some(seek_time) = request.seek_time
            && let Err(err) = player.seek(seek_time)
        {
            did_seek = false;
            tracing::warn!("failed to seek: {err:?}");
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

    info!("Loading mcap summary");

    let mcap_player = McapPlayer::new(&args.file)?;
    let (start_time, end_time) = mcap_player.time_range();

    let mcap_player = Arc::new(Mutex::new(mcap_player));
    let listener = Arc::new(Listener::new(mcap_player.clone()));

    let server = WebSocketServer::new()
        .name(file_name)
        .capabilities([Capability::PlaybackControl, Capability::Time])
        .playback_time_range(start_time, end_time)
        .listener(listener)
        .bind(&args.host, args.port)
        .start_blocking()
        .expect("Server failed to start");

    info!("Waiting for client");
    std::thread::sleep(Duration::from_secs(1));

    info!("Starting stream");
    let mut last_status = PlaybackStatus::Paused;
    while !done.load(Ordering::Relaxed) {
        let status = {
            let player = mcap_player.lock().unwrap();
            let status = player.status();

            // Broadcast state change when playback ends
            if status == PlaybackStatus::Ended && last_status != PlaybackStatus::Ended {
                server.broadcast_playback_state(PlaybackState {
                    current_time: player.current_time(),
                    playback_speed: player.playback_speed(),
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
        let sleep_duration = mcap_player.lock().unwrap().log_next_message(&server)?;
        if let Some(duration) = sleep_duration {
            // Upper-bound sleep time to avoid the player from becoming unresponsive for too long
            std::thread::sleep(std::cmp::min(duration, Duration::from_secs(1)));
        }
    }

    server.stop().wait_blocking();
    Ok(())
}
