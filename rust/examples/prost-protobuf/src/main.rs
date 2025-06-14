use foxglove::{LazyChannel, McapWriteOptions, McapWriter};

mod protos;

static APPLE_CHANNEL: LazyChannel<protos::Apple> = LazyChannel::new("/apple");

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let options = McapWriteOptions::new();
    let writer = McapWriter::with_options(options)
        .create_new_buffered_file("example.mcap")
        .expect("Failed to start mcap writer");

    APPLE_CHANNEL.log(&protos::Apple {
        color: Some("red".to_string()),
        diameter: Some(10),
    });

    writer.close().expect("Failed to flush mcap file");
}
