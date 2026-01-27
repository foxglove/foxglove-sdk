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
use foxglove::{
    ChannelBuilder, PartialMetadata, RawChannel, Schema, WebSocketServer, WebSocketServerHandle,
};
use mcap::records::MessageHeader;
use mcap::sans_io::indexed_reader::{IndexedReadEvent, IndexedReader, IndexedReaderOptions};
use mcap::sans_io::summary_reader::{SummaryReadEvent, SummaryReader, SummaryReaderOptions};
use mcap::Summary as McapSummary;
use tracing::info;

struct Inner {
    status: PlaybackStatus,
    current_time: u64,
    pending_seek_time: Option<u64>,
    playback_speed: f32,
    time_tracker: Option<TimeTracker>,
}

struct StreamMcapListener {
    inner: Mutex<Inner>,
}

impl StreamMcapListener {
    fn new(summary: &Summary) -> Self {
        Self {
            inner: Mutex::new(Inner {
                status: PlaybackStatus::Paused,
                current_time: summary.start_time,
                pending_seek_time: None,
                playback_speed: 1.0,
                time_tracker: None,
            }),
        }
    }

    fn is_playing(&self) -> bool {
        self.inner.lock().unwrap().status == PlaybackStatus::Playing
    }

    fn take_seek_request(&self) -> Option<u64> {
        self.inner.lock().unwrap().pending_seek_time.take()
    }

    fn update_status(&self, status: PlaybackStatus) {
        self.inner.lock().unwrap().status = status;
    }

    fn current_state(&self) -> PlaybackState {
        let inner = self.inner.lock().unwrap();
        PlaybackState {
            status: inner.status,
            current_time: inner.current_time,
            playback_speed: inner.playback_speed,
            did_seek: false,
            request_id: None,
        }
    }

    fn reset_time_tracker(&self) {
        self.inner.lock().unwrap().time_tracker = None;
    }

    /// Sleeps until the specified log time, pacing playback to match the playback speed.
    fn sleep_until_log_time(&self, log_time: u64) {
        let sleep_duration = {
            let mut inner = self.inner.lock().unwrap();
            let speed = inner.playback_speed;
            let tt = inner
                .time_tracker
                .get_or_insert_with(|| TimeTracker::start(log_time));
            tt.compute_sleep_duration(log_time, speed)
        };

        if sleep_duration >= Duration::from_micros(1) {
            std::thread::sleep(sleep_duration);
        }

        let mut inner = self.inner.lock().unwrap();
        inner.current_time = log_time;
        if let Some(tt) = &mut inner.time_tracker {
            tt.update_position(log_time);
        }
    }

    /// Returns a timestamp if it's time to broadcast the current time to clients.
    fn should_broadcast_time(&self) -> Option<u64> {
        self.inner.lock().unwrap().time_tracker.as_mut()?.notify()
    }
}

impl ServerListener for StreamMcapListener {
    fn on_playback_control_request(
        &self,
        request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        let mut inner = self.inner.lock().unwrap();

        // Reset time tracker if speed changed or seeking
        let speed_changed = (request.playback_speed - inner.playback_speed).abs() > f32::EPSILON;
        if speed_changed || request.seek_time.is_some() {
            inner.time_tracker = None;
        }

        inner.playback_speed = request.playback_speed.max(0.01);
        inner.status = match request.playback_command {
            PlaybackCommand::Play => PlaybackStatus::Playing,
            PlaybackCommand::Pause => PlaybackStatus::Paused,
        };

        info!(
            "Handled requested playback command {:?}",
            request.playback_command
        );

        let did_seek = request.seek_time.is_some();
        if let Some(seek_time) = request.seek_time {
            info!("Requested seek to {}ns", seek_time);
            inner.pending_seek_time = Some(seek_time);
            inner.current_time = seek_time;
        }

        Some(PlaybackState {
            status: inner.status,
            current_time: inner.current_time,
            playback_speed: inner.playback_speed,
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

    let listener = Arc::new(StreamMcapListener::new(&summary));

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
            listener,
            summary,
        }
    }

    /// Streams the file content until `done` is set.
    fn stream_until(
        self,
        server: &WebSocketServerHandle,
        done: &Arc<AtomicBool>,
    ) -> Result<()> {
        let mut file = File::open(&self.path).context("open MCAP for streaming")?;
        let mut chunk_buf = Vec::new();
        let mut reader = self.build_reader(None)?;
        while !done.load(Ordering::Relaxed) {
            if let Some(seek_time) = self.listener.take_seek_request() {
                reader = self.build_reader(Some(seek_time))?;
                continue;
            }

            if !self.listener.is_playing() {
                self.listener.reset_time_tracker();
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
    fn handle_message(&self, server: &WebSocketServerHandle, header: MessageHeader, data: &[u8]) {
        self.listener.sleep_until_log_time(header.log_time);

        if let Some(timestamp) = self.listener.should_broadcast_time() {
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
