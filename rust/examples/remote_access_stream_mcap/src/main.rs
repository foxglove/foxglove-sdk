//! Streams an MCAP file through the remote access gateway, looping playback
//! with inter-message timing preserved.

use std::{
    collections::HashMap,
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use foxglove::{ChannelBuilder, RawChannel, Schema, remote_access::Gateway};
use mcap::Summary;
use mcap::sans_io::indexed_reader::{IndexedReadEvent, IndexedReader, IndexedReaderOptions};
use mcap::sans_io::summary_reader::{SummaryReadEvent, SummaryReader};

#[derive(Parser)]
struct Args {
    /// Path to an MCAP file to play back in a loop.
    #[arg(long)]
    file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Args::parse();

    let handle = Gateway::new()
        .start()
        .expect("Failed to start remote access gateway");

    let result = tokio::select! {
        r = mcap_loop(args.file) => r,
        _ = tokio::signal::ctrl_c() => Ok(()),
    };

    _ = handle.stop().await;

    result
}

/// Reads and loops an MCAP file, publishing its messages as though they were live data.
async fn mcap_loop(path: PathBuf) -> Result<()> {
    loop {
        mcap_playback(&path)
            .await
            .inspect_err(|e| eprintln!("MCAP playback error: {e:#}"))?;

        println!("MCAP playback complete, looping...");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Plays through an MCAP file once, respecting inter-message timing.
async fn mcap_playback(path: &Path) -> Result<()> {
    let mut file = BufReader::new(std::fs::File::open(path)?);
    let summary = load_summary(&mut file)?.ok_or_else(|| anyhow!("missing summary section"))?;

    let stats = summary
        .stats
        .as_ref()
        .ok_or_else(|| anyhow!("MCAP summary missing stats record"))?;

    let channels = create_channels(&summary)?;

    let mut reader = IndexedReader::new_with_options(
        &summary,
        IndexedReaderOptions::new().log_time_on_or_after(stats.message_start_time),
    )
    .map_err(|e| anyhow!("failed to create indexed reader: {e}"))?;

    let mut chunk_buffer = Vec::new();
    let mut first_log_time: Option<u64> = None;
    let mut base_wall_time: Option<tokio::time::Instant> = None;

    loop {
        match reader.next_event() {
            None => return Ok(()),
            Some(Err(e)) => return Err(anyhow!("indexed reader error: {e}")),
            Some(Ok(IndexedReadEvent::ReadChunkRequest { offset, length })) => {
                file.seek(SeekFrom::Start(offset))
                    .context("seek to chunk")?;
                chunk_buffer.resize(length, 0);
                file.read_exact(&mut chunk_buffer).context("read chunk")?;
                reader
                    .insert_chunk_record_data(offset, &chunk_buffer)
                    .map_err(|e| anyhow!("failed to insert chunk: {e}"))?;
            }
            Some(Ok(IndexedReadEvent::Message { header, data })) => {
                let first = *first_log_time.get_or_insert(header.log_time);
                let base = *base_wall_time.get_or_insert_with(tokio::time::Instant::now);

                let log_offset = Duration::from_nanos(header.log_time.saturating_sub(first));
                let target = base + log_offset;
                tokio::time::sleep_until(target).await;

                if let Some(channel) = channels.get(&header.channel_id) {
                    channel.log(data);
                }
            }
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
        let schema = mcap_channel
            .schema
            .as_ref()
            .map(|s| Schema::new(s.name.as_str(), s.encoding.as_str(), s.data.to_vec()));
        let channel = ChannelBuilder::new(&mcap_channel.topic)
            .message_encoding(&mcap_channel.message_encoding)
            .schema(schema)
            .build_raw()?;
        channels.insert(id, channel);
    }
    Ok(channels)
}
