use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::{Parser, ValueEnum};
use foxglove::{BTreeMap, LazyChannel, McapCompression, McapWriteOptions, McapWriter};
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

fn verify_metadata(path: &PathBuf) {
    use std::fs;
    use std::collections::HashMap;

    match fs::read(path) {
        Ok(contents) => {
            use mcap::read::LinearReader;

            // Simple table of expected metadata - easy to modify
            let expected: &[(&str, &[(&str, &str)])] = &[
                ("test1", &[("key1", "value1"), ("key2", "value2")]),
                ("test2", &[("a", "1"), ("b", "2")]),
                ("test3", &[("x", "y"), ("z", "w")]),
            ];

            let mut found_metadata: HashMap<String, std::collections::BTreeMap<String, String>> = HashMap::new();
            let mut metadata_count = 0;
            let mut found_empty_test = false;

            // Read all metadata from file
            for record in LinearReader::new(&contents).unwrap() {
                if let Ok(mcap::records::Record::Metadata(metadata)) = record {
                    metadata_count += 1;
                    found_metadata.insert(metadata.name.clone(), metadata.metadata.clone());

                    if metadata.name == "empty_test" {
                        found_empty_test = true;
                    }
                }
            }

            // Verify count
            assert_eq!(metadata_count, expected.len(), "Wrong number of metadata records");

            // Verify each expected metadata exists with correct key-value pairs
            for (name, expected_kv) in expected {
                let actual = found_metadata.get(*name)
                    .unwrap_or_else(|| panic!("Metadata '{}' not found", name));

                let expected_map: std::collections::BTreeMap<String, String> = expected_kv.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();

                assert_eq!(actual, &expected_map, "Metadata '{}' has wrong key-value pairs", name);
            }

            // Verify empty metadata was skipped
            assert!(!found_empty_test, "Empty metadata should not have been written");

            println!("All metadata tests passed!");
        }
        Err(e) => panic!("Failed to read file: {}", e),
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

    // Test 1: Write basic metadata
    let mut metadata1 = BTreeMap::new();
    metadata1.insert("key1".to_string(), "value1".to_string());
    metadata1.insert("key2".to_string(), "value2".to_string());

    writer
        .write_metadata("test1", metadata1)
        .expect("Failed to write metadata");

    // Test 2: Write multiple metadata records
    let mut metadata2 = BTreeMap::new();
    metadata2.insert("a".to_string(), "1".to_string());
    metadata2.insert("b".to_string(), "2".to_string());

    writer
        .write_metadata("test2", metadata2)
        .expect("Failed to write metadata2");

    let mut metadata3 = BTreeMap::new();
    metadata3.insert("x".to_string(), "y".to_string());
    metadata3.insert("z".to_string(), "w".to_string());

    writer
        .write_metadata("test3", metadata3)
        .expect("Failed to write metadata3");

    // Test 3: Write empty metadata (should be skipped)
    let empty_metadata = BTreeMap::new();
    writer
        .write_metadata("empty_test", empty_metadata)
        .expect("Failed to write empty metadata");

    log_until(args.fps, done);
    writer.close().expect("Failed to flush mcap file");

    // Verify metadata was written
    println!("Verifying metadata in output file...");
    verify_metadata(&args.path);
}
