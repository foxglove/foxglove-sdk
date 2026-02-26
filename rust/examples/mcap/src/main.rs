use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{Parser, ValueEnum};
use foxglove::{LazyChannel, McapCompression, McapWriteOptions, McapWriter};
use std::time::Duration;

#[derive(Debug, Parser)]
struct Cli {
    /// Output path.
    #[arg(short, long, default_value = "output.mcap")]
    path: PathBuf,
    /// If set, overwrite an existing file.
    #[arg(long)]
    overwrite: bool,
    /// Chunk size.
    #[arg(long, default_value_t = 1024 * 768)]
    chunk_size: u64,
    /// Compression algorithm to use.
    #[arg(long, default_value = "zstd")]
    compression: CompressionArg,
    /// Frames per second.
    #[arg(long, default_value_t = 10)]
    fps: u8,
}

#[derive(Debug, Clone, ValueEnum)]
enum CompressionArg {
    Zstd,
    Lz4,
    None,
}
impl From<CompressionArg> for Option<McapCompression> {
    fn from(value: CompressionArg) -> Self {
        match value {
            CompressionArg::Zstd => Some(McapCompression::Zstd),
            CompressionArg::Lz4 => Some(McapCompression::Lz4),
            CompressionArg::None => None,
        }
    }
}

#[derive(Debug, foxglove::Encode)]
struct Message {
    msg: String,
    count: u32,
}

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct JsonMessage {
    msg: String,
    count: u32,
}

static MSG_CHANNEL: LazyChannel<Message> = LazyChannel::new("/msg");
static JSON_CHANNEL: LazyChannel<JsonMessage> = LazyChannel::new("/json");

fn log_until(fps: u8, stop: Arc<AtomicBool>) {
    let mut count: u32 = 0;
    let duration = Duration::from_millis(1000 / u64::from(fps));
    while !stop.load(Ordering::Relaxed) {
        MSG_CHANNEL.log(&Message {
            msg: "Hello, world!".to_string(),
            count,
        });
        JSON_CHANNEL.log(&JsonMessage {
            msg: "Hello, JSON!".to_string(),
            count,
        });
        std::thread::sleep(duration);
        count += 1;
    }
}

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    let done = Arc::new(AtomicBool::default());
    ctrlc::set_handler({
        let done = done.clone();
        move || {
            done.store(true, Ordering::Relaxed);
        }
    })
    .expect("Failed to set SIGINT handler");

    if args.overwrite && args.path.exists() {
        std::fs::remove_file(&args.path).expect("Failed to remove file");
    }

    let options = McapWriteOptions::new()
        .chunk_size(Some(args.chunk_size))
        .compression(args.compression.into());

    let writer = McapWriter::with_options(options)
        .create_new_buffered_file(&args.path)
        .expect("Failed to start mcap writer");

    // If you want to add some MCAP metadata: https://mcap.dev/spec#metadata-op0x0c
    let mut metadata = BTreeMap::new();
    metadata.insert("os".to_string(), std::env::consts::OS.to_string());
    metadata.insert("arch".to_string(), std::env::consts::ARCH.to_string());
    writer
        .write_metadata("platform", metadata)
        .expect("Failed to write metadata");

    log_until(args.fps, done);
    writer.close().expect("Failed to flush mcap file");
}
