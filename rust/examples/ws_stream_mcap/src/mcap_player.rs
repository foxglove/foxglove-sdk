use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use foxglove::websocket::PlaybackStatus;
use foxglove::{ChannelBuilder, PartialMetadata, RawChannel, Schema, WebSocketServerHandle};
use mcap::sans_io::indexed_reader::{IndexedReadEvent, IndexedReader, IndexedReaderOptions};
use mcap::sans_io::summary_reader::{SummaryReadEvent, SummaryReader};
use mcap::Summary;

use crate::playback_source::PlaybackSource;

pub struct McapPlayer {
    summary: Summary,
    channels: HashMap<u16, Arc<RawChannel>>,
    reader: IndexedReader,
    file: BufReader<File>,
    chunk_buffer: Vec<u8>,
    time_tracker: Option<TimeTracker>,
    time_range: (u64, u64),
    status: PlaybackStatus,
    current_time: u64,
    playback_speed: f32,
}

impl McapPlayer {
    /// Creates a new MCAP player.
    pub(crate) fn new(path: &Path) -> Result<Self> {
        let mut file = BufReader::new(File::open(path)?);

        // Read the summary using SummaryReader
        let summary = load_summary(&mut file)?.ok_or_else(|| anyhow!("missing summary section"))?;

        let stats = summary
            .stats
            .as_ref()
            .ok_or_else(|| anyhow!("MCAP summary section missing stats record"))?;

        let time_range = (stats.message_start_time, stats.message_end_time);
        let current_time = stats.message_start_time;

        // Create foxglove channels from the summary
        let channels = create_channels(&summary)?;

        // Create the indexed reader
        let reader = IndexedReader::new_with_options(
            &summary,
            IndexedReaderOptions::new().log_time_on_or_after(current_time),
        )
        .map_err(|e| anyhow!("failed to create indexed reader: {e}"))?;

        Ok(Self {
            time_range,
            current_time,
            status: PlaybackStatus::Paused,
            playback_speed: 1.0,
            summary,
            channels,
            reader,
            file,
            chunk_buffer: Vec::new(),
            time_tracker: None,
        })
    }

    /// Re-creates the indexed reader starting from the given time.
    fn reset_reader(&mut self, start_time: u64) -> Result<()> {
        self.reader = IndexedReader::new_with_options(
            &self.summary,
            IndexedReaderOptions::new().log_time_on_or_after(start_time),
        )
        .map_err(|e| anyhow!("failed to create indexed reader: {e}"))?;
        self.time_tracker = None;
        Ok(())
    }

    /// Processes reader events until a message is available or EOF.
    /// Returns the next message header and data, or None if no more messages.
    fn next_message(&mut self) -> Result<Option<(mcap::records::MessageHeader, Vec<u8>)>> {
        loop {
            match self.reader.next_event() {
                None => return Ok(None),
                Some(Err(e)) => return Err(anyhow!("indexed reader error: {e}")),
                Some(Ok(IndexedReadEvent::ReadChunkRequest { offset, length })) => {
                    self.file
                        .seek(SeekFrom::Start(offset))
                        .context("seek to chunk")?;
                    self.chunk_buffer.resize(length, 0);
                    self.file
                        .read_exact(&mut self.chunk_buffer)
                        .context("read chunk")?;
                    self.reader
                        .insert_chunk_record_data(offset, &self.chunk_buffer)
                        .map_err(|e| anyhow!("failed to insert chunk: {e}"))?;
                }
                Some(Ok(IndexedReadEvent::Message { header, data })) => {
                    return Ok(Some((header, data.to_vec())));
                }
            }
        }
    }
}

impl PlaybackSource for McapPlayer {
    fn time_range(&self) -> (u64, u64) {
        self.time_range
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

    fn set_playback_speed(&mut self, speed: f32) {
        if let Some(tt) = &mut self.time_tracker {
            tt.set_speed(speed);
        }
        self.playback_speed = speed;
    }

    fn play(&mut self) {
        if let Some(tt) = &mut self.time_tracker {
            tt.resume();
        }
        self.status = PlaybackStatus::Playing;
    }

    fn pause(&mut self) {
        if let Some(tt) = &mut self.time_tracker {
            tt.pause();
        }
        self.status = PlaybackStatus::Paused;
    }

    fn seek(&mut self, log_time: u64) -> Result<()> {
        let log_time = log_time.clamp(self.time_range.0, self.time_range.1);
        self.current_time = log_time;
        self.reset_reader(log_time)?;
        Ok(())
    }

    fn next_wakeup(&mut self) -> Option<Instant> {
        if self.status != PlaybackStatus::Playing {
            return None;
        }

        // Peek at the next message to determine when we should wake up
        // We can't actually peek without consuming, so we return a short interval
        // and let log_messages handle the actual timing
        let tt = self.time_tracker.as_ref()?;
        Some(tt.next_wakeup())
    }

    fn log_messages(&mut self, server: &WebSocketServerHandle) -> Result<()> {
        if self.status != PlaybackStatus::Playing {
            return Ok(());
        }

        // Process messages that are ready to be logged based on elapsed time
        loop {
            let Some((header, data)) = self.next_message()? else {
                // No more messages, playback has ended
                self.status = PlaybackStatus::Paused;
                self.current_time = self.time_range.1;
                return Ok(());
            };

            let tt = self.time_tracker.get_or_insert_with(|| {
                TimeTracker::start(header.log_time, self.playback_speed)
            });

            // Check if we've reached a message that's in the future
            if !tt.is_ready(header.log_time) {
                // Put the message back by resetting the reader to the current position
                // This is inefficient but simple - a production implementation would buffer
                self.reset_reader(header.log_time)?;
                break;
            }

            // Update current time
            self.current_time = header.log_time;

            // Broadcast time update periodically
            if let Some(timestamp) = tt.notify(header.log_time) {
                server.broadcast_time(timestamp);
            }

            // Log the message to the appropriate channel
            if let Some(channel) = self.channels.get(&header.channel_id) {
                channel.log_with_meta(
                    &data,
                    PartialMetadata {
                        log_time: Some(header.log_time),
                    },
                );
            }
        }

        Ok(())
    }
}

/// Helper for keeping track of the relationship between a file timestamp and the wallclock.
struct TimeTracker {
    /// Wall-clock time when playback started/resumed
    start: Instant,
    /// Log time corresponding to the start instant
    offset_ns: u64,
    /// Current playback speed multiplier
    speed: f32,
    /// Whether playback is paused
    paused: bool,
    /// Elapsed log time when paused
    paused_elapsed_ns: u64,
    /// Interval for time broadcast notifications
    notify_interval_ns: u64,
    /// Last log time that was broadcast
    notify_last: u64,
}

impl TimeTracker {
    /// Initializes a new time tracker, treating "now" as the specified log time.
    fn start(offset_ns: u64, speed: f32) -> Self {
        Self {
            start: Instant::now(),
            offset_ns,
            speed,
            paused: false,
            paused_elapsed_ns: 0,
            notify_interval_ns: 1_000_000_000 / 60,
            notify_last: 0,
        }
    }

    /// Returns the current playback log time based on elapsed wall time and speed.
    fn current_log_time(&self) -> u64 {
        if self.paused {
            self.offset_ns + self.paused_elapsed_ns
        } else {
            let elapsed_wall = self.start.elapsed();
            let elapsed_log_ns = (elapsed_wall.as_nanos() as f64 * self.speed as f64) as u64;
            self.offset_ns + self.paused_elapsed_ns + elapsed_log_ns
        }
    }

    /// Returns true if the given log time is ready to be played.
    fn is_ready(&self, log_time: u64) -> bool {
        log_time <= self.current_log_time()
    }

    /// Returns the next wall-clock instant to wake up.
    fn next_wakeup(&self) -> Instant {
        // Wake up frequently to check for new messages
        Instant::now() + Duration::from_millis(1)
    }

    /// Pauses playback, recording the current elapsed time.
    fn pause(&mut self) {
        if !self.paused {
            let elapsed_wall = self.start.elapsed();
            let elapsed_log_ns = (elapsed_wall.as_nanos() as f64 * self.speed as f64) as u64;
            self.paused_elapsed_ns += elapsed_log_ns;
            self.paused = true;
        }
    }

    /// Resumes playback from the paused position.
    fn resume(&mut self) {
        if self.paused {
            self.start = Instant::now();
            self.paused = false;
        }
    }

    /// Updates the playback speed.
    fn set_speed(&mut self, speed: f32) {
        if !self.paused {
            // Accumulate elapsed time at the old speed before changing
            let elapsed_wall = self.start.elapsed();
            let elapsed_log_ns = (elapsed_wall.as_nanos() as f64 * self.speed as f64) as u64;
            self.paused_elapsed_ns += elapsed_log_ns;
            self.start = Instant::now();
        }
        self.speed = speed;
    }

    /// Periodically returns a timestamp reference to broadcast to clients.
    fn notify(&mut self, current_ns: u64) -> Option<u64> {
        if current_ns.saturating_sub(self.notify_last) >= self.notify_interval_ns {
            self.notify_last = current_ns;
            Some(current_ns)
        } else {
            None
        }
    }
}

/// Loads the MCAP summary using the sans-io SummaryReader.
fn load_summary<R: Read + Seek>(file: &mut R) -> Result<Option<Summary>> {
    let mut reader = SummaryReader::new();
    while let Some(event) = reader.next_event() {
        match event.map_err(|e| anyhow!("summary read error: {e}"))? {
            SummaryReadEvent::ReadRequest(n) => {
                let read = file.read(reader.insert(n)).context("read summary")?;
                reader.notify_read(read);
            }
            SummaryReadEvent::SeekRequest(pos) => {
                let pos = file.seek(pos).context("seek summary")?;
                reader.notify_seeked(pos);
            }
        }
    }
    Ok(reader.finish())
}

/// Creates foxglove channels from the MCAP summary.
fn create_channels(summary: &Summary) -> Result<HashMap<u16, Arc<RawChannel>>> {
    let mut channels = HashMap::new();
    for (&id, mcap_channel) in &summary.channels {
        let schema = mcap_channel.schema.as_ref().map(|s| {
            Schema::new(s.name.as_str(), s.encoding.as_str(), s.data.to_vec())
        });
        let channel = ChannelBuilder::new(&mcap_channel.topic)
            .message_encoding(&mcap_channel.message_encoding)
            .schema(schema)
            .build_raw()?;
        channels.insert(id, channel);
    }
    Ok(channels)
}
