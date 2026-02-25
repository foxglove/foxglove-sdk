//! This example demonstrates how to use the Foxglove SDK to filter messages when logging to an MCAP
//! file and/or a WebSocket server.
//!
//! Oftentimes, you may want to split "heavy" topics out into separate MCAP recordings, but still
//! log everything for live visualization. Splitting on topic in this way can be useful for
//! selectively retrieving data from bandwidth-constrained environments, such as with the Foxglove
//! Agent.
//!
//! Note that if you just want to partition topics into different MCAPs without live visualization,
//! you could instead set up different logging `Context`s.
//!
//! In this example, we log some point cloud data to one MCAP file, and some minimal metadata to
//! another.
use foxglove::schemas::{
    packed_element_field::NumericType, PackedElementField, PointCloud, Pose, Quaternion, Vector3,
};
use foxglove::schemas::{FrameTransform, FrameTransforms};
use foxglove::{Encode, LazyChannel, McapWriter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const SMALL_MCAP_FILE: &str = "example-topic-splitting-small.mcap";
const LARGE_MCAP_FILE: &str = "example-topic-splitting-large.mcap";

#[derive(Encode)]
struct Message {
    state: String,
}

static INFO_CHANNEL: LazyChannel<Message> = LazyChannel::new("/info");
static POINT_CLOUD_CHANNEL: LazyChannel<PointCloud> = LazyChannel::new("/point_cloud");
static POINT_CLOUD_TF_CHANNEL: LazyChannel<FrameTransforms> = LazyChannel::new("/point_cloud_tf");

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

    // In one MCAP, drop all of our point_cloud (and related tf) messages
    let small_mcap = McapWriter::new()
        .channel_filter_fn(|channel| !channel.topic().starts_with("/point_cloud"))
        .create_new_buffered_file(SMALL_MCAP_FILE)
        .expect("Failed to create mcap writer");

    // In the other, log only the point_cloud (and related tf) messages
    let large_mcap = McapWriter::new()
        .channel_filter_fn(|channel| channel.topic().starts_with("/point_cloud"))
        .create_new_buffered_file(LARGE_MCAP_FILE)
        .expect("Failed to create mcap writer");

    // We'll send all messages to a running app. We don't need a filter, since it's the same as
    // having no filter applied, but this demonstrates how to add one to the WS server.
    foxglove::WebSocketServer::new()
        .channel_filter_fn(|_| true)
        .start_blocking()
        .expect("Server failed to start");

    let start = SystemTime::now();
    let cloud_tf = FrameTransforms {
        transforms: vec![FrameTransform {
            parent_frame_id: "world".to_string(),
            child_frame_id: "points".to_string(),
            translation: Some(Vector3 {
                x: -10.0,
                y: -10.0,
                z: 0.0,
            }),
            ..Default::default()
        }],
    };

    while !done.load(Ordering::Relaxed) {
        let elapsed = SystemTime::now()
            .duration_since(start)
            .expect("Time went backwards");

        let state = get_state(elapsed);
        INFO_CHANNEL.log(&Message { state });

        let point_cloud = make_point_cloud(elapsed);
        POINT_CLOUD_CHANNEL.log(&point_cloud);
        POINT_CLOUD_TF_CHANNEL.log(&cloud_tf);

        std::thread::sleep(Duration::from_millis(33));
    }

    small_mcap.close().expect("Failed to close mcap");
    large_mcap.close().expect("Failed to close mcap");
}

fn get_state(elapsed: Duration) -> String {
    let t = elapsed.as_secs_f32().cos();
    if t > 0.0 {
        "pos".to_string()
    } else {
        "neg".to_string()
    }
}

/// Generate an example point cloud.
///
/// Adapted from <https://foxglove.dev/blog/visualizing-point-clouds-with-custom-colors>
fn make_point_cloud(elapsed: Duration) -> PointCloud {
    let t = elapsed.as_secs_f32();
    let mut points = Vec::new();
    for x in 0..20 {
        for y in 0..20 {
            let x_coord = x as f32 + (t + y as f32 / 5.0).cos();
            let y_coord = y as f32;
            let z_coord = 0.0f32;

            let r = (255.0 * (0.5 + 0.5 * x_coord / 20.0)) as u8;
            let g = (255.0 * y_coord / 20.0) as u8;
            let b = (255.0 * (0.5 + 0.5 * t.sin())) as u8;
            let a = (255.0 * (0.5 + 0.5 * ((x_coord / 20.0) * (y_coord / 20.0)))) as u8;

            points.push((x_coord, y_coord, z_coord, r, g, b, a));
        }
    }

    // Pack data into bytes
    let mut buffer = Vec::new();
    for (x, y, z, r, g, b, a) in points {
        buffer.extend_from_slice(&x.to_le_bytes());
        buffer.extend_from_slice(&y.to_le_bytes());
        buffer.extend_from_slice(&z.to_le_bytes());
        buffer.push(r);
        buffer.push(g);
        buffer.push(b);
        buffer.push(a);
    }

    // Create fields defining the data structure
    let fields = vec![
        PackedElementField {
            name: "x".to_string(),
            offset: 0,
            r#type: NumericType::Float32.into(),
        },
        PackedElementField {
            name: "y".to_string(),
            offset: 4,
            r#type: NumericType::Float32.into(),
        },
        PackedElementField {
            name: "z".to_string(),
            offset: 8,
            r#type: NumericType::Float32.into(),
        },
        PackedElementField {
            name: "rgba".to_string(),
            offset: 12,
            r#type: NumericType::Uint32.into(),
        },
    ];

    PointCloud {
        timestamp: None,
        frame_id: "points".to_string(),
        pose: Some(Pose {
            position: Some(Vector3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            }),
            orientation: Some(Quaternion {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            }),
        }),
        point_stride: 16, // 4 fields * 4 bytes
        fields,
        data: buffer.into(),
    }
}
