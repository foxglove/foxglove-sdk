//! Streams an mcap file over a websocket.

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
use clap::Parser;
use foxglove::websocket::Capability;
use foxglove::{
    ChannelBuilder, PartialMetadata, RawChannel, Schema, WebSocketServer, WebSocketServerHandle,
};
use mcap::records::{MessageHeader, Record, SchemaHeader};
use mcap::sans_io::linear_reader::{LinearReadEvent, LinearReader, LinearReaderOptions};
use tracing::info;

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

    let server = WebSocketServer::new()
        .name(file_name)
        .capabilities([Capability::Time])
        .bind(&args.host, args.port)
        .start_blocking()
        .expect("Server failed to start");

    info!("Loading mcap summary");
    let summary = Summary::load_from_mcap(&args.file)?;

    info!("Waiting for client");
    std::thread::sleep(Duration::from_secs(1));

    info!("Starting stream");
    while !done.load(Ordering::Relaxed) {
        summary.file_stream().stream_until(&server, &done)?;
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

#[derive(Default)]
struct Summary {
    path: PathBuf,
    schemas: HashMap<u16, Schema>,
    channels: HashMap<u16, Arc<RawChannel>>,
}

impl Summary {
    fn load_from_mcap(path: &Path) -> Result<Self> {
        let mut file = BufReader::new(File::open(path)?);

        // Read the last 28 bytes of the file to validate the trailing magic (8 bytes) and obtain
        // the summary start value, which is the first u64 in the footer record (20 bytes).
        let mut buf = Vec::with_capacity(28);
        file.seek(SeekFrom::End(-28)).context("seek footer")?;
        file.read_to_end(&mut buf).context("read footer")?;
        if !buf.ends_with(mcap::MAGIC) {
            return Err(anyhow!("bad footer magic"));
        }

        // Seek to summary section.
        let summary_start = buf.as_slice().get_u64_le();
        if summary_start == 0 {
            return Err(anyhow!("missing summary section"));
        }
        file.seek(SeekFrom::Start(summary_start))
            .context("seek summary")?;

        let mut reader = LinearReader::new_with_options(LinearReaderOptions {
            skip_start_magic: true,
            ..Default::default()
        });

        let mut summary = Summary {
            path: path.to_owned(),
            schemas: HashMap::new(),
            channels: HashMap::new(),
        };
        while advance_reader(&mut reader, &mut file, |rec| summary.handle_record(rec))
            .context("read summary")?
        {}

        Ok(summary)
    }

    /// Creates a new file stream.
    fn file_stream(&self) -> FileStream<'_> {
        FileStream::new(&self.path, &self.channels)
    }

    // Handles a record from the summary section.
    fn handle_record(&mut self, record: Record<'_>) -> Result<()> {
        match record {
            Record::Schema { header, data } => self.handle_schema(&header, data),
            Record::Channel(channel) => self.handle_channel(channel),
            _ => Ok(()),
        }
    }

    /// Caches schema information.
    fn handle_schema(
        &mut self,
        header: &SchemaHeader,
        data: Cow<'_, [u8]>,
    ) -> Result<(), anyhow::Error> {
        if header.id == 0 {
            return Err(anyhow!("invalid schema id"))?;
        }
        if let Entry::Vacant(entry) = self.schemas.entry(header.id) {
            let schema = Schema::new(&header.name, &header.encoding, data.into_owned());
            entry.insert(schema);
        }
        Ok(())
    }

    /// Registers a new channel.
    fn handle_channel(&mut self, record: mcap::records::Channel) -> Result<(), anyhow::Error> {
        if let Entry::Vacant(entry) = self.channels.entry(record.id) {
            let schema = self.schemas.get(&record.schema_id).cloned();
            let channel = ChannelBuilder::new(record.topic)
                .message_encoding(&record.message_encoding)
                .schema(schema)
                .build_raw()?;
            entry.insert(channel);
        }
        Ok(())
    }
}

struct FileStream<'a> {
    path: PathBuf,
    channels: &'a HashMap<u16, Arc<RawChannel>>,
    time_tracker: Option<TimeTracker>,
}

impl<'a> FileStream<'a> {
    /// Creates a new file stream.
    fn new(path: &Path, channels: &'a HashMap<u16, Arc<RawChannel>>) -> Self {
        Self {
            path: path.to_owned(),
            channels,
            time_tracker: None,
        }
    }

    /// Streams the file content until `done` is set.
    fn stream_until(
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
