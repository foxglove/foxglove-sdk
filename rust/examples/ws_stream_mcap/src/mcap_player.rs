use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use foxglove::websocket::PlaybackStatus;
use foxglove::{ChannelBuilder, PartialMetadata, RawChannel, Schema};
use mcap::records::MessageHeader;
use mcap::sans_io::indexed_reader::{IndexedReadEvent, IndexedReader, IndexedReaderOptions};
use mcap::sans_io::summary_reader::{SummaryReadEvent, SummaryReader, SummaryReaderOptions};
use mcap::Summary as McapSummary;

use crate::playback_source::PlaybackSource;

struct PendingMessage {
    header: MessageHeader,
    data: Vec<u8>,
}

pub struct McapPlayer {
    file: File,
    channels: HashMap<u16, Arc<RawChannel>>,
    summary: Arc<McapSummary>,
    reader: IndexedReader,
    chunk_buf: Vec<u8>,
    playback_speed: f32,
    current_time: u64,
    status: PlaybackStatus,
    time_tracker: Option<TimeTracker>,
    pending: Option<PendingMessage>,
}

impl McapPlayer {
    pub fn new(path: &Path) -> Result<Self> {
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
            pending: None,
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

    fn read_next_message(&mut self) -> Result<Option<PendingMessage>> {
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
                    return Ok(Some(PendingMessage {
                        header,
                        data: data.to_vec(),
                    }));
                }
            }
        }
    }

    fn log_message(&self, header: MessageHeader, data: &[u8]) {
        if let Some(channel) = self.channels.get(&header.channel_id) {
            channel.log_with_meta(
                data,
                PartialMetadata {
                    log_time: Some(header.log_time),
                },
            );
        }
    }

    fn log_until(&mut self, log_time: u64) -> Result<()> {
        if let Some(pending) = self.pending.take() {
            if pending.header.log_time <= log_time {
                self.log_message(pending.header, &pending.data);
            } else {
                self.pending = Some(pending);
                return Ok(());
            }
        }

        loop {
            let Some(message) = self.read_next_message()? else {
                return Ok(());
            };

            if message.header.log_time > log_time {
                self.pending = Some(message);
                return Ok(());
            }

            self.log_message(message.header, &message.data);
        }
    }

    fn reset_timebase(&mut self) {
        self.time_tracker = None;
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
        self.reset_timebase();
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

    fn seek(&mut self, log_time: u64) -> Result<()> {
        self.reader = Self::build_reader(self.summary.as_ref(), Some(log_time))?;
        self.current_time = log_time;
        self.pending = None;
        self.reset_timebase();
        Ok(())
    }

    fn next_wakeup(&mut self) -> Result<Option<Instant>> {
        if self.pending.is_none() {
            self.pending = self.read_next_message()?;
        }

        let Some(pending) = &self.pending else {
            self.status = PlaybackStatus::Ended;
            return Ok(None);
        };

        let tt = self
            .time_tracker
            .get_or_insert_with(|| TimeTracker::start(pending.header.log_time));
        let sleep_duration =
            tt.compute_sleep_duration(pending.header.log_time, self.playback_speed);
        let wakeup = if sleep_duration > Duration::from_micros(0) {
            Instant::now() + sleep_duration
        } else {
            Instant::now()
        };

        Ok(Some(wakeup))
    }

    fn flush_since_last(&mut self) -> Result<()> {
        let anchor_time = self
            .pending
            .as_ref()
            .map(|pending| pending.header.log_time)
            .unwrap_or(self.current_time);
        let current_log_time = {
            let tt = self
                .time_tracker
                .get_or_insert_with(|| TimeTracker::start(anchor_time));
            tt.current_log_time(self.playback_speed)
        };
        self.log_until(current_log_time)?;
        if let Some(tt) = &mut self.time_tracker {
            tt.update_position(current_log_time);
        }
        self.current_time = current_log_time;
        Ok(())
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

    /// Estimates the current log time based on wall clock and playback speed.
    fn current_log_time(&self, playback_speed: f32) -> u64 {
        let elapsed_ns = self.start.elapsed().as_nanos() as f64;
        let scaled_ns = elapsed_ns * (playback_speed as f64);
        self.start_log_ns.saturating_add(scaled_ns as u64)
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
