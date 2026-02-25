use foxglove::{
    convert::SaturatingInto,
    log,
    schemas::{
        Color, CubePrimitive, Log, Pose, Quaternion, SceneEntity, SceneUpdate, Timestamp, Vector3,
    },
    ChannelBuilder, McapWriter,
};
use std::time::Duration;

#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
struct Message {
    msg: String,
    count: u32,
}

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let writer = McapWriter::new()
        .create_new_buffered_file("example.mcap")
        .expect("Failed to start mcap writer");

    // You can use log! with a topic name and any type that implements Encode, including foxglove schemas
    // without having to create a channel first.
    log!(
        "/log",
        Log {
            timestamp: Some(Timestamp::now()),
            level: 1,
            message: "Hello, world!".to_string(),
            name: "main".to_string(),
            file: "main.rs".to_string(),
            line: 42,
        },
        // You can specify an optional `log_time` "keyword" argument
        log_time = 1000,
    );

    // Including custom structs that implement serde::Serialize and schemars::JsonSchema
    log!(
        "/msg",
        Message {
            msg: "Hello, world!".to_string(),
            count: 42,
        }
    );

    // You can also use log! with existing channels in the default Context, as long as the message encoding and schema match
    let _channel = ChannelBuilder::new("/scene").build::<SceneUpdate>();

    log!(
        "/scene",
        SceneUpdate {
            deletions: vec![],
            entities: vec![SceneEntity {
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
                            x: 0.0,
                            y: 0.0,
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
            }],
        }
    );

    writer.close().expect("Failed to flush mcap file");
}
