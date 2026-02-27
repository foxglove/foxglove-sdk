use std::ops::Add;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use foxglove::schemas::{Color, CubePrimitive, SceneEntity, SceneUpdate, Vector3};
use foxglove::{LazyChannel, LazyRawChannel, McapWriter};

const FILE_NAME: &str = "quickstart-rust.mcap";

// Our example logs data on a couple of different topics, so we'll create a
// channel for each. We can use a channel like Channel<SceneUpdate> to log
// Foxglove schemas, or a generic RawChannel to log custom data.
static SCENE: LazyChannel<SceneUpdate> = LazyChannel::new("/scene");
static SIZE: LazyRawChannel = LazyRawChannel::new("/size", "json");

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let done = Arc::new(AtomicBool::default());
    ctrlc::set_handler({
        let done = done.clone();
        move || {
            done.store(true, Ordering::Relaxed);
        }
    })
    .expect("Failed to set SIGINT handler");

    // We'll log to both an MCAP file, and to a running Foxglove app via a server.
    let mcap = McapWriter::new()
        .create_new_buffered_file(FILE_NAME)
        .expect("Failed to start mcap writer");

    // Start a server to communicate with the Foxglove app. This will run indefinitely, even if
    // references are dropped.
    foxglove::WebSocketServer::new()
        .start_blocking()
        .expect("Server failed to start");

    while !done.load(Ordering::Relaxed) {
        let size = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
            .sin()
            .abs()
            .add(1.0);

        // Log messages on the channel until interrupted. By default, each message
        // is stamped with the current time.
        SIZE.log(format!("{{\"size\": {size}}}").as_bytes());
        SCENE.log(&SceneUpdate {
            deletions: vec![],
            entities: vec![SceneEntity {
                id: "box".to_string(),
                cubes: vec![CubePrimitive {
                    size: Some(Vector3 {
                        x: size,
                        y: size,
                        z: size,
                    }),
                    color: Some(Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        });

        std::thread::sleep(std::time::Duration::from_millis(33));
    }

    mcap.close().expect("Failed to close mcap writer");
}
