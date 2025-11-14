//! Streams an mcap file over a websocket.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use foxglove::websocket::{
    Capability, PlaybackCommand, PlaybackControlRequest, PlaybackState, PlaybackStatus,
    ServerListener,
};
use foxglove::{
    ChannelBuilder, PartialMetadata, RawChannel, Schema, WebSocketServer, WebSocketServerHandle,
};
use mcap::records::MessageHeader;
use mcap::sans_io::indexed_reader::{IndexedReadEvent, IndexedReader, IndexedReaderOptions};
use mcap::sans_io::summary_reader::{SummaryReadEvent, SummaryReader, SummaryReaderOptions};
use mcap::Summary as McapSummary;
use tracing::info;

struct StreamMcapListener {
    status: Mutex<PlaybackStatus>,
    current_time: AtomicU64,
    pending_seek_time: AtomicU64,
    has_pending_seek: AtomicBool,
    playback_speed: AtomicU32,
}

impl StreamMcapListener {
    fn new() -> Self {
        Self {
            status: Mutex::new(PlaybackStatus::Paused),
            current_time: AtomicU64::new(0),
            pending_seek_time: AtomicU64::new(0),
            has_pending_seek: AtomicBool::new(false),
            playback_speed: AtomicU32::new(1.0f32.to_bits()),
        }
    }

    fn is_playing(&self) -> bool {
        *self.status.lock().unwrap() == PlaybackStatus::Playing
    }

    fn update_current_time(&self, timestamp: u64) {
        self.current_time.store(timestamp, Ordering::Relaxed);
    }

    fn request_seek(&self, timestamp: u64) {
        info!("Requested seek to {}ns", timestamp);
        self.pending_seek_time.store(timestamp, Ordering::Relaxed);
        self.has_pending_seek.store(true, Ordering::Release);
    }

    fn take_seek_request(&self) -> Option<u64> {
        if self
            .has_pending_seek
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            Some(self.pending_seek_time.load(Ordering::Relaxed))
        } else {
            None
        }
    }

    fn playback_speed(&self) -> f32 {
        f32::from_bits(self.playback_speed.load(Ordering::Relaxed))
    }

    fn update_playback_speed(&self, speed: f32) {
        self.playback_speed
            .store(speed.max(0.01).to_bits(), Ordering::Relaxed);
    }

    fn handle_command(&self, command: PlaybackCommand) {
        match command {
            PlaybackCommand::Play => self.update_status(PlaybackStatus::Playing),
            PlaybackCommand::Pause => self.update_status(PlaybackStatus::Paused),
        }
    }

    fn update_status(&self, status: PlaybackStatus) {
        *self.status.lock().unwrap() = status;
    }

    fn status(&self) -> PlaybackStatus {
        *self.status.lock().unwrap()
    }

    fn current_state(&self) -> PlaybackState {
        PlaybackState {
            status: self.status(),
            current_time: self.current_time.load(Ordering::Relaxed),
            playback_speed: self.playback_speed(),
            request_id: None,
        }
    }
}

impl ServerListener for StreamMcapListener {
    fn on_playback_control_request(&self, request: PlaybackControlRequest) -> PlaybackState {
        self.update_playback_speed(request.playback_speed);
        self.handle_command(request.playback_command);

        info!(
            "Handled requested playback command {:?}",
            request.playback_command
        );

        if let Some(seek_time) = request.seek_time {
            self.request_seek(seek_time);
            self.update_current_time(seek_time);
        }

        PlaybackState {
            request_id: Some(request.request_id),
            ..self.current_state()
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
    let summary = Summary::load_from_mcap(&args.file)?;

    info!(
        "Found time bounds in mcap file: ({}, {})",
        summary.start_time, summary.end_time
    );

    let listener = Arc::new(StreamMcapListener::new());

    let server = WebSocketServer::new()
        .name(file_name)
        .capabilities([Capability::Time])
        .listener(listener.clone())
        .playback_time_range(summary.start_time, summary.end_time)
        .bind(&args.host, args.port)
        .start_blocking()
        .expect("Server failed to start");

    info!("Waiting for client");
    std::thread::sleep(Duration::from_secs(1));

    info!("Starting stream");
    while !done.load(Ordering::Relaxed) {
        if listener.current_state().status != PlaybackStatus::Ended {
            summary
                .file_stream(listener.clone())
                .stream_until(&server, &done)?;

            info!("Playback complete");
            listener.update_status(PlaybackStatus::Ended);
            server.broadcast_playback_state(PlaybackState {
                ..listener.current_state()
            });
        }
    }

    server.stop().wait_blocking();
    Ok(())
}

#[derive(Default)]
struct Summary {
    path: PathBuf,
    channels: HashMap<u16, Arc<RawChannel>>,
    start_time: u64,
    end_time: u64,
    mcap_summary: Arc<McapSummary>,
}

impl Summary {
    fn load_from_mcap(path: &Path) -> Result<Self> {
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
        let mcap_summary = reader
            .finish()
            .ok_or_else(|| anyhow!("missing summary section"))?;
        let stats = mcap_summary.stats.as_ref().ok_or_else(|| {
            anyhow!("MCAP summary missing statistics record for playback time bounds")
        })?;
        let channels = build_raw_channels(&mcap_summary)?;
        Ok(Self {
            path: path.to_owned(),
            channels,
            start_time: stats.message_start_time,
            end_time: stats.message_end_time,
            mcap_summary: Arc::new(mcap_summary),
        })
    }

    /// Creates a new file stream.
    fn file_stream(&self, listener: Arc<StreamMcapListener>) -> FileStream<'_> {
        FileStream::new(
            &self.path,
            &self.channels,
            listener,
            self.mcap_summary.clone(),
        )
    }
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

struct FileStream<'a> {
    path: PathBuf,
    channels: &'a HashMap<u16, Arc<RawChannel>>,
    time_tracker: Option<TimeTracker>,
    listener: Arc<StreamMcapListener>,
    summary: Arc<McapSummary>,
}

impl<'a> FileStream<'a> {
    /// Creates a new file stream.
    fn new(
        path: &Path,
        channels: &'a HashMap<u16, Arc<RawChannel>>,
        listener: Arc<StreamMcapListener>,
        summary: Arc<McapSummary>,
    ) -> Self {
        Self {
            path: path.to_owned(),
            channels,
            time_tracker: None,
            listener,
            summary,
        }
    }

    /// Streams the file content until `done` is set.
    fn stream_until(
        mut self,
        server: &WebSocketServerHandle,
        done: &Arc<AtomicBool>,
    ) -> Result<()> {
        let mut file = File::open(&self.path).context("open MCAP for streaming")?;
        let mut chunk_buf = Vec::new();
        let mut reader = self.build_reader(None)?;
        while !done.load(Ordering::Relaxed) {
            if let Some(seek_time) = self.listener.take_seek_request() {
                reader = self.build_reader(Some(seek_time))?;
                self.time_tracker = None;
                continue;
            }

            if !self.listener.is_playing() {
                self.time_tracker = None;
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }

            let event = match reader.next_event() {
                Some(Ok(event)) => event,
                Some(Err(err)) => return Err(err.into()),
                None => break,
            };

            match event {
                IndexedReadEvent::ReadChunkRequest { offset, length } => {
                    chunk_buf.resize(length, 0);
                    file.seek(SeekFrom::Start(offset))
                        .context("seek chunk data")?;
                    file.read_exact(&mut chunk_buf).context("read chunk data")?;
                    reader
                        .insert_chunk_record_data(offset, &chunk_buf)
                        .context("insert chunk data")?;
                }
                IndexedReadEvent::Message { header, data } => {
                    self.handle_message(server, header, data);
                }
            }
        }
        Ok(())
    }

    fn build_reader(&self, start: Option<u64>) -> Result<IndexedReader> {
        let options = IndexedReaderOptions {
            start,
            ..Default::default()
        };
        IndexedReader::new_with_options(self.summary.as_ref(), options)
            .context("create indexed reader")
    }

    /// Streams the message data to the server.
    fn handle_message(
        &mut self,
        server: &WebSocketServerHandle,
        header: MessageHeader,
        data: &[u8],
    ) {
        let speed = self.listener.playback_speed();
        let tt = self
            .time_tracker
            .get_or_insert_with(|| TimeTracker::start(header.log_time, speed));

        tt.sleep_until(header.log_time, speed);
        self.listener.update_current_time(header.log_time);

        if let Some(timestamp) = tt.notify() {
            server.broadcast_time(timestamp);
        }

        if let Some(channel) = self.channels.get(&header.channel_id) {
            channel.log_with_meta(
                data,
                PartialMetadata {
                    log_time: Some(header.log_time),
                },
            );
        }
    }
}

/// Helper for keep tracking of the relationship between a file timestamp and the wallclock.
struct TimeTracker {
    start: Instant,
    start_log_ns: u64,
    playback_speed: f32,
    now_ns: u64,
    notify_interval_ns: u64,
    notify_last: u64,
}
impl TimeTracker {
    /// Initializes a new time tracker, treating "now" as the specified offset from epoch.
    fn start(offset_ns: u64, playback_speed: f32) -> Self {
        Self {
            start: Instant::now(),
            start_log_ns: offset_ns,
            playback_speed,
            now_ns: offset_ns,
            notify_interval_ns: 1_000_000_000 / 60,
            notify_last: 0,
        }
    }

    /// Sleeps until the specified offset.
    fn sleep_until(&mut self, offset_ns: u64, playback_speed: f32) {
        if (playback_speed - self.playback_speed).abs() > f32::EPSILON {
            self.start = Instant::now();
            self.start_log_ns = offset_ns;
            self.playback_speed = playback_speed;
            self.now_ns = offset_ns;
            return;
        }

        let delta_log = offset_ns.saturating_sub(self.start_log_ns);
        let scaled = Duration::from_nanos(delta_log).mul_f64(1.0 / (self.playback_speed as f64));
        let delta = scaled.saturating_sub(self.start.elapsed());
        if delta >= Duration::from_micros(1) {
            std::thread::sleep(delta);
        }
        self.now_ns = offset_ns;
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
