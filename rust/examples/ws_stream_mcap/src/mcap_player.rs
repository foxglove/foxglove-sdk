use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use bytes::Buf;
use foxglove::websocket::PlaybackStatus;
use foxglove::{ChannelBuilder, PartialMetadata, RawChannel, Schema, WebSocketServerHandle};
use mcap::read::Summary;
use mcap::records::{MessageHeader, Record, SchemaHeader};
use mcap::sans_io::linear_reader::{LinearReadEvent, LinearReader, LinearReaderOptions};

use crate::playback_source::PlaybackSource;

pub struct McapPlayer {
    contents: Vec<u8>,
    path: PathBuf,
    summary: Summary,
    time_tracker: Option<TimeTracker>,
}

impl McapPlayer {
    /// Creates a new MCAP player.
    pub(crate) fn new(path: &Path) -> Result<Self> {
        let contents = std::fs::read(&path)?;
        let summary = Summary::read(&contents)
            .context("failed to read MCAP summary")?
            .ok_or_else(|| anyhow!("MCAP file has no summary section"))?;
        Ok(Self {
            contents,
            path: path.to_owned(),
            summary,
            time_tracker: None,
        })
    }

    /// Streams the file content until `done` is set.
    pub fn stream_until(
        mut self,
        server: &WebSocketServerHandle,
        done: &Arc<AtomicBool>,
    ) -> Result<()> {
        let mut file = BufReader::new(File::open(&self.path)?);
        let mut reader = LinearReader::new();
        while !done.load(Ordering::Relaxed)
            && advance_reader(&mut reader, &mut file, |rec| {
                self.handle_record(server, rec);
                Ok(())
            })
            .context("read data")?
        {}
        Ok(())
    }

    /// Handles an mcap record parsed from the file.
    fn handle_record(&mut self, server: &WebSocketServerHandle, record: Record<'_>) {
        if let Record::Message { header, data } = record {
            self.handle_message(server, header, &data);
        }
    }

    /// Streams the message data to the server.
    fn handle_message(
        &mut self,
        server: &WebSocketServerHandle,
        header: MessageHeader,
        data: &[u8],
    ) {
        let tt = self
            .time_tracker
            .get_or_insert_with(|| TimeTracker::start(header.log_time));

        tt.sleep_until(header.log_time);

        if let Some(timestamp) = tt.notify() {
            server.broadcast_time(timestamp);
        }

        if let Some(channel) = self.summary.channels.get(&header.channel_id) {
            channel.log_with_meta(
                data,
                PartialMetadata {
                    log_time: Some(header.log_time),
                },
            );
        }
    }
}

impl PlaybackSource for McapPlayer {
    fn time_bounds(&self) -> (u64, u64) {
        todo!()
    }

    fn set_playback_speed(&mut self, speed: f32) {
        todo!()
    }

    fn play(&mut self) {
        todo!()
    }

    fn pause(&mut self) {
        todo!()
    }

    fn seek(&mut self, log_time: u64) -> Result<()> {
        todo!()
    }

    fn status(&self) -> PlaybackStatus {
        todo!()
    }

    fn current_time(&self) -> u64 {
        todo!()
    }

    fn playback_speed(&self) -> f32 {
        todo!()
    }

    fn next_wakeup(&mut self) -> Option<Instant> {
        todo!()
    }

    fn log_messages(&mut self, server: &WebSocketServerHandle) -> Result<()> {
        todo!()
    }
}

/// Helper for keep tracking of the relationship between a file timestamp and the wallclock.
struct TimeTracker {
    start: Instant,
    offset_ns: u64,
    now_ns: u64,
    notify_interval_ns: u64,
    notify_last: u64,
}
impl TimeTracker {
    /// Initializes a new time tracker, treating "now" as the specified offset from epoch.
    fn start(offset_ns: u64) -> Self {
        Self {
            start: Instant::now(),
            offset_ns,
            now_ns: offset_ns,
            notify_interval_ns: 1_000_000_000 / 60,
            notify_last: 0,
        }
    }

    /// Sleeps until the specified offset.
    fn sleep_until(&mut self, offset_ns: u64) {
        let abs = Duration::from_nanos(offset_ns.saturating_sub(self.offset_ns));
        let delta = abs.saturating_sub(self.start.elapsed());
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

/// Helper function to advance the mcap reader.
fn advance_reader<R, F>(
    reader: &mut LinearReader,
    file: &mut R,
    mut handle_record: F,
) -> Result<bool>
where
    R: Read + Seek,
    F: FnMut(Record<'_>) -> Result<()>,
{
    if let Some(event) = reader.next_event() {
        match event? {
            LinearReadEvent::ReadRequest(count) => {
                let count = file.read(reader.insert(count))?;
                reader.notify_read(count);
            }
            LinearReadEvent::Record { data, opcode } => {
                let record = mcap::parse_record(opcode, data)?;
                handle_record(record)?;
            }
        }
        Ok(true)
    } else {
        Ok(false)
    }
}
