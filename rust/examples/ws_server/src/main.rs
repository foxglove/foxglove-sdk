use clap::Parser;
use foxglove::convert::SaturatingInto;
use foxglove::schemas::{
    Color, CubePrimitive, FrameTransform, Pose, Quaternion, SceneEntity, SceneUpdate, Vector3,
};

use foxglove::{LazyChannel, LazyRawChannel};
use std::time::Duration;

#[derive(Debug, Clone, Copy, foxglove::Encode)]
enum MessageLevel {
    Debug,
    #[allow(dead_code)]
    Info,
}

#[derive(foxglove::Encode)]
struct Message {
    level: MessageLevel,
    msg: String,
    count: u32,
}

static BOX_CHANNEL: LazyChannel<SceneUpdate> = LazyChannel::new("/boxes");
static TF_CHANNEL: LazyChannel<FrameTransform> = LazyChannel::new("/tf");
static MSG_CHANNEL: LazyChannel<Message> = LazyChannel::new("/msg");

// Foxglove supports logging arbitrary JSON values without specifying a schema
static SCHEMALESS_CHANNEL: LazyRawChannel = LazyRawChannel::new("/schemaless", "json");

async fn log_forever(fps: u8) {
    let mut counter: u32 = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(1000 / u64::from(fps)));
    loop {
        interval.tick().await;
        log(counter);
        counter += 1;
    }
}

fn log(counter: u32) {
    MSG_CHANNEL.log(&Message {
        level: MessageLevel::Debug,
        msg: "Hello, world!".to_string(),
        count: counter,
    });

    BOX_CHANNEL.log(&SceneUpdate {
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
                    orientation: Some(euler_to_quaternion(0.0, 0.0, f64::from(counter) * -0.1)),
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
    });

    TF_CHANNEL.log(&FrameTransform {
        parent_frame_id: "world".to_string(),
        child_frame_id: "box".to_string(),
        rotation: Some(euler_to_quaternion(1.0, 0.0, f64::from(counter) * 0.1)),
        ..Default::default()
    });

    SCHEMALESS_CHANNEL.log(format!("{{\"count\": {counter}}}").as_bytes());
}

#[derive(Debug, Parser)]
struct Cli {
    /// Server TCP port.
    #[arg(short, long, default_value_t = 8765)]
    port: u16,
    /// Server IP address.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// Frames per second.
    #[arg(long, default_value_t = 60)]
    fps: u8,
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    let server = foxglove::WebSocketServer::new()
        .name(env!("CARGO_PKG_NAME"))
        .bind(&args.host, args.port)
        .start()
        .await
        .expect("Server failed to start");

    let app_url = server.app_url();
    println!("View in browser: {app_url}");
    println!("View in desktop: {}", app_url.with_open_in_desktop());

    tokio::task::spawn(log_forever(args.fps));
    _ = tokio::signal::ctrl_c().await;
    server.stop().wait().await;
}

fn euler_to_quaternion(roll: f64, pitch: f64, yaw: f64) -> Quaternion {
    let quat = quaternion::euler_angles(roll, pitch, yaw);
    Quaternion {
        x: quat.1[0],
        y: quat.1[1],
        z: quat.1[2],
        w: quat.0,
    }
}
