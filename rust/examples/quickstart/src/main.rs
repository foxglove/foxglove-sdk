use std::{
    ops::Add,
    sync::atomic::{AtomicBool, Ordering},
};

use foxglove::McapWriter;

const FILE_NAME: &str = "quickstart-rust.mcap";

foxglove::static_typed_channel!(pub(crate) SCENE, "/scene", foxglove::schemas::SceneUpdate);

fn log_message() {
    let size = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        .sin()
        .abs()
        .add(1.0);

    SCENE.log(&foxglove::schemas::SceneUpdate {
        deletions: vec![],
        entities: vec![foxglove::schemas::SceneEntity {
            id: "box".to_string(),
            cubes: vec![foxglove::schemas::CubePrimitive {
                size: Some(foxglove::schemas::Vector3 {
                    x: size,
                    y: size,
                    z: size,
                }),
                color: Some(foxglove::schemas::Color {
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
}

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    static DONE: AtomicBool = AtomicBool::new(false);

    ctrlc::set_handler(move || {
        DONE.store(true, Ordering::Release);
    })
    .expect("Failed to set SIGINT handler");

    foxglove::WebSocketServer::new()
        .start_blocking()
        .expect("Server failed to start");

    let mcap = McapWriter::new()
        .create_new_buffered_file(FILE_NAME)
        .expect("Failed to start mcap writer");

    while !DONE.load(Ordering::Acquire) {
        log_message();
        std::thread::sleep(std::time::Duration::from_millis(33));
    }

    mcap.close().expect("Failed to close mcap writer");
}
