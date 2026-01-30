//! Streams an mcap file over a websocket.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus,
    ServerListener,
};
use foxglove::{ChannelBuilder, PartialMetadata, RawChannel, Schema, WebSocketServer};
use mcap::sans_io::indexed_reader::{IndexedReadEvent, IndexedReader, IndexedReaderOptions};
use mcap::sans_io::summary_reader::{SummaryReadEvent, SummaryReader, SummaryReaderOptions};
use mcap::Summary as McapSummary;
use tracing::{info, warn};

struct PlaybackMessage {
    channel_id: u16,
    log_time: u64,
    data: Vec<u8>,
}

trait PlaybackSource {
    fn time_bounds(&self) -> (u64, u64);
    fn set_playback_speed(&mut self, speed: f32);
    fn play(&mut self);
    fn pause(&mut self);
    fn status(&self) -> PlaybackStatus;
    fn set_status(&mut self, status: PlaybackStatus);
    fn seek(&mut self, log_time: u64) -> Result<()>;
    fn next_message(&mut self) -> Result<Option<PlaybackMessage>>;
    fn reset_timebase(&mut self);
    fn sleep_until_log_time(&mut self, log_time: u64);
    fn should_broadcast_time(&mut self) -> Option<u64>;
    fn current_time(&self) -> u64;
    fn playback_speed(&self) -> f32;
    fn channels(&self) -> &HashMap<u16, Arc<RawChannel>>;
}

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
                playback.reset_timebase();
                did_seek = true;
            }
        }

        if (request.playback_speed - playback.playback_speed()).abs() > f32::EPSILON {
            playback.set_playback_speed(request.playback_speed);
            playback.reset_timebase();
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

struct McapPlayer {
    file: File,
    channels: HashMap<u16, Arc<RawChannel>>,
    summary: Arc<McapSummary>,
    reader: IndexedReader,
    chunk_buf: Vec<u8>,
    playback_speed: f32,
    current_time: u64,
    status: PlaybackStatus,
    time_tracker: Option<TimeTracker>,
}

impl McapPlayer {
    fn new(path: &Path) -> Result<Self> {
        let mcap_summary = Self::load_mcap_summary(path)?;
        let channels = Self::build_raw_channels(&mcap_summary)?;
        let start_time = mcap_summary
            .stats
            .as_ref()
            .ok_or_else(|| anyhow!("MCAP summary missing statistics record"))?
            .message_start_time;
        let file = File::open(path).context("open MCAP for streaming")?;
        let reader = Self::build_reader(&mcap_summary, None)?;
        Ok(Self {
            file,
            channels,
            summary: Arc::new(mcap_summary),
            reader,
            chunk_buf: Vec::new(),
            playback_speed: 1.0,
            current_time: start_time,
            status: PlaybackStatus::Paused,
            time_tracker: None,
        })
    }

    fn build_reader(summary: &McapSummary, start: Option<u64>) -> Result<IndexedReader> {
        let options = IndexedReaderOptions {
            start,
            ..Default::default()
        };
        IndexedReader::new_with_options(summary, options).context("create indexed reader")
    }

    fn build_raw_channels(summary: &McapSummary) -> Result<HashMap<u16, Arc<RawChannel>>> {
        summary
            .channels
            .iter()
            .map(|(id, channel)| {
                let schema = channel.schema.as_ref().map(|schema| {
                    Schema::new(
                        &schema.name,
                        &schema.encoding,
                        schema.data.clone().into_owned(),
                    )
                });
                let raw_channel = ChannelBuilder::new(&channel.topic)
                    .message_encoding(&channel.message_encoding)
                    .schema(schema)
                    .build_raw()
                    .context("build raw channel")?;
                Ok((*id, raw_channel))
            })
            .collect()
    }

    fn load_mcap_summary(path: &Path) -> Result<McapSummary> {
        let mut file = File::open(path).context("open MCAP file")?;
        let file_size = file.metadata().context("stat MCAP file")?.len();
        let mut reader = SummaryReader::new_with_options(
            SummaryReaderOptions::default().with_file_size(file_size),
        );
        while let Some(event) = reader.next_event() {
            match event.context("read summary event")? {
                SummaryReadEvent::ReadRequest(count) => {
                    let buf = reader.insert(count);
                    let read = file.read(buf).context("read summary data")?;
                    if read == 0 {
                        return Err(anyhow!("unexpected EOF while reading summary section"));
                    }
                    reader.notify_read(read);
                }
                SummaryReadEvent::SeekRequest(seek_to) => {
                    let pos = file.seek(seek_to).context("seek summary data")?;
                    reader.notify_seeked(pos);
                }
            }
        }
        reader
            .finish()
            .ok_or_else(|| anyhow!("missing summary section"))
    }
}

impl PlaybackSource for McapPlayer {
    fn time_bounds(&self) -> (u64, u64) {
        let stats = self
            .summary
            .stats
            .as_ref()
            .expect("MCAP summary missing statistics record");
        (stats.message_start_time, stats.message_end_time)
    }

    fn set_playback_speed(&mut self, speed: f32) {
        self.playback_speed = speed.max(0.01);
    }

    fn play(&mut self) {
        self.status = PlaybackStatus::Playing;
    }

    fn pause(&mut self) {
        self.status = PlaybackStatus::Paused;
        self.reset_timebase();
    }

    fn status(&self) -> PlaybackStatus {
        self.status
    }

    fn set_status(&mut self, status: PlaybackStatus) {
        self.status = status;
    }

    fn seek(&mut self, log_time: u64) -> Result<()> {
        self.reader = Self::build_reader(self.summary.as_ref(), Some(log_time))?;
        self.current_time = log_time;
        Ok(())
    }

    fn next_message(&mut self) -> Result<Option<PlaybackMessage>> {
        loop {
            let event = match self.reader.next_event() {
                Some(Ok(event)) => event,
                Some(Err(err)) => return Err(err.into()),
                None => return Ok(None),
            };

            match event {
                IndexedReadEvent::ReadChunkRequest { offset, length } => {
                    self.chunk_buf.resize(length, 0);
                    self.file
                        .seek(SeekFrom::Start(offset))
                        .context("seek chunk data")?;
                    self.file
                        .read_exact(&mut self.chunk_buf)
                        .context("read chunk data")?;
                    self.reader
                        .insert_chunk_record_data(offset, &self.chunk_buf)
                        .context("insert chunk data")?;
                }
                IndexedReadEvent::Message { header, data } => {
                    return Ok(Some(PlaybackMessage {
                        channel_id: header.channel_id,
                        log_time: header.log_time,
                        data: data.to_vec(),
                    }));
                }
            }
        }
    }

    fn reset_timebase(&mut self) {
        self.time_tracker = None;
    }

    /// Sleeps until the specified log time, pacing playback to match the playback speed.
    fn sleep_until_log_time(&mut self, log_time: u64) {
        let sleep_duration = {
            let tt = self
                .time_tracker
                .get_or_insert_with(|| TimeTracker::start(log_time));
            tt.compute_sleep_duration(log_time, self.playback_speed)
        };

        if sleep_duration >= Duration::from_micros(1) {
            std::thread::sleep(sleep_duration);
        }

        if let Some(tt) = &mut self.time_tracker {
            tt.update_position(log_time);
        }

        self.current_time = log_time;
    }

    /// Returns a timestamp if it's time to broadcast the current time to clients.
    fn should_broadcast_time(&mut self) -> Option<u64> {
        self.time_tracker.as_mut()?.notify()
    }

    fn current_time(&self) -> u64 {
        self.current_time
    }

    fn playback_speed(&self) -> f32 {
        self.playback_speed
    }

    fn channels(&self) -> &HashMap<u16, Arc<RawChannel>> {
        &self.channels
    }
}

/// Helper for tracking the relationship between a file timestamp and the wallclock.
struct TimeTracker {
    start: Instant,
    start_log_ns: u64,
    now_ns: u64,
    notify_interval_ns: u64,
    notify_last: u64,
}

impl TimeTracker {
    /// Initializes a new time tracker, treating "now" as the specified log time.
    fn start(log_time: u64) -> Self {
        Self {
            start: Instant::now(),
            start_log_ns: log_time,
            now_ns: log_time,
            notify_interval_ns: 1_000_000_000 / 60,
            notify_last: 0,
        }
    }

    /// Computes how long to sleep to pace playback to the given log time.
    fn compute_sleep_duration(&self, log_time: u64, playback_speed: f32) -> Duration {
        let delta_log = log_time.saturating_sub(self.start_log_ns);
        let scaled = Duration::from_nanos(delta_log).mul_f64(1.0 / (playback_speed as f64));
        scaled.saturating_sub(self.start.elapsed())
    }

    /// Updates the current position after sleeping.
    fn update_position(&mut self, log_time: u64) {
        self.now_ns = log_time;
    }

    /// Periodically returns a timestamp reference to broadcast to clients.
    fn notify(&mut self) -> Option<u64> {
        if self.now_ns.saturating_sub(self.notify_last) >= self.notify_interval_ns {
            self.notify_last = self.now_ns;
            Some(self.now_ns)
        } else {
            None
        }
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

        let (message, timestamp, channel) = {
            let mut player = player.lock().unwrap();
            let message = player.next_message()?;
            let mut timestamp = None;
            let mut channel = None;
            if let Some(msg) = &message {
                player.sleep_until_log_time(msg.log_time);
                timestamp = player.should_broadcast_time();
                channel = player.channels().get(&msg.channel_id).cloned();
            }
            (message, timestamp, channel)
        };

        match message {
            Some(msg) => {
                if let Some(timestamp) = timestamp {
                    server.broadcast_time(timestamp);
                }

                if let Some(channel) = channel {
                    channel.log_with_meta(
                        &msg.data,
                        PartialMetadata {
                            log_time: Some(msg.log_time),
                        },
                    );
                }
            }
            None => {
                info!("Playback complete");
                {
                    let mut player = player.lock().unwrap();
                    player.set_status(PlaybackStatus::Ended);
                }
                server.broadcast_playback_state(PlaybackState {
                    ..listener.current_state()
                });
            }
        }
    }

    server.stop().wait_blocking();
    Ok(())
}
