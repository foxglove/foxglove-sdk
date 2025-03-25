use std::path::PathBuf;

use clap::Parser;
use foxglove::{McapWriter, TypedChannel};
use mcap::{Compression, WriteOptions};

pub mod protos {
    include!(concat!(env!("OUT_DIR"), "/custom.rs"));
}

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
}

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    if args.overwrite && args.path.exists() {
        std::fs::remove_file(&args.path).expect("Failed to remove file");
    }

    let options = WriteOptions::new()
        .chunk_size(Some(args.chunk_size))
        .compression(Some(Compression::Zstd));

    let writer = McapWriter::with_options(options)
        .create_new_buffered_file(&args.path)
        .expect("Failed to start mcap writer");

    let channel = TypedChannel::new("/msg").unwrap();

    channel.log(&protos::CustomMessage {
        msg: "Hello, world!".to_string(),
        count: 0,
    });

    writer.close().expect("Failed to flush mcap file");
}
