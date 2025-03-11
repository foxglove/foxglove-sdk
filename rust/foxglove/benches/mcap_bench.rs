use std::time::Duration;

use foxglove::convert::SaturatingInto;
use foxglove::schemas::log::Level;
use foxglove::schemas::{
    Color, CubePrimitive, Log, Pose, Quaternion, SceneEntity, SceneUpdate, Timestamp, Vector3,
};
use foxglove::McapWriter;

fn main() {
    // Run registered benchmarks.
    divan::main();
}

const PRINTABLE: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

#[divan::bench(args = [50, 100, 200, 400])]
fn log_json_message(bencher: divan::Bencher, num_chars: usize) {
    #[derive(Debug, serde::Serialize, schemars::JsonSchema)]
    struct Message {
        msg: &'static str,
        count: u32,
    }

    foxglove::static_typed_channel!(pub MSG_CHANNEL, "/msg", Message);

    let tmpdir = tempfile::tempdir().unwrap();
    let path = tmpdir.path().join("test.mcap");
    let _writer = McapWriter::new()
        .create_new_buffered_file(&path)
        .expect("Failed to start mcap writer");

    let message = Message {
        msg: "warmup",
        count: num_chars as u32,
    };
    MSG_CHANNEL.log(&message);

    bencher.bench(|| {
        for _ in 0..100 {
            let message = Message {
                msg: &PRINTABLE[..num_chars],
                count: (num_chars as u32) << 12,
            };
            MSG_CHANNEL.log(&message);
        }
    });
}

#[divan::bench(args = [1, 2, 4, 8])]
fn log_scene_update(bencher: divan::Bencher, num_entities: usize) {
    foxglove::static_typed_channel!(pub SCENE_CHANNEL, "/boxes", SceneUpdate);

    let tmpdir = tempfile::tempdir().unwrap();
    let path = tmpdir.path().join("test.mcap");
    let _writer = McapWriter::new()
        .create_new_buffered_file(&path)
        .expect("Failed to start mcap writer");

    bencher.bench(|| {
        let mut entities = Vec::with_capacity(num_entities);
        for i in 0..num_entities {
            entities.push(SceneEntity {
                frame_id: "box".to_string(),
                id: "box_1".to_string(),
                lifetime: Some(Duration::from_millis(10_100).saturating_into()),
                cubes: vec![CubePrimitive {
                    pose: Some(Pose {
                        position: Some(Vector3 {
                            x: 0.0,
                            y: 0.0,
                            z: 3.0,
                        }),
                        orientation: Some(Quaternion {
                            x: i as f64 * -0.1,
                            y: i as f64 * 0.1,
                            z: 0.0,
                            w: 1.0,
                        }),
                    }),
                    size: Some(Vector3 {
                        x: 1.0,
                        y: 1.0,
                        z: 1.0,
                    }),
                    color: Some(Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                }],
                ..Default::default()
            });
        }

        for _ in 0..100 {
            SCENE_CHANNEL.log(&SceneUpdate {
                deletions: vec![],
                entities: entities.clone(),
            });
        }
    });
}

#[divan::bench(args = [1, 2, 4, 8, 16, 32])]
fn mutlithreaded_log(bencher: divan::Bencher, num_threads: usize) {
    foxglove::static_typed_channel!(pub LOG_CHANNEL, "/logs", Log);

    let tmpdir = tempfile::tempdir().unwrap();
    let path = tmpdir.path().join("test.mcap");
    let _writer = McapWriter::new()
        .create_new_buffered_file(&path)
        .expect("Failed to start mcap writer");

    bencher.bench(|| {
        let mut threads = Vec::with_capacity(num_threads);
        for _ in 0..num_threads {
            let handle = std::thread::spawn(move || {
                for _ in 0..4096 / num_threads {
                    LOG_CHANNEL.log(&Log {
                        timestamp: Some(Timestamp::new(1234567890, 123456789)),
                        level: Level::Info as i32,
                        message: PRINTABLE[..100].to_string(),
                        name: "node name".to_string(),
                        file: "file_name.rs".to_string(),
                        line: 1111,
                    });
                }
            });
            threads.push(handle);
        }

        for thread in threads {
            thread.join().unwrap();
        }
    });
}
