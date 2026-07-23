//! Remote access point cloud compression demo.
//!
//! Streams the same animated point cloud on two topics: `/cloud/compressed` uses the
//! remote access sink's transparent Draco compression (on by default), while `/cloud/raw`
//! opts out via `suppress_point_cloud_compression_fn` and is delivered unmodified.
//!
//! Lossy remote access messages larger than the data-track cap (100 KiB by default) are
//! dropped before publishing. At the default `--points 25000`, the raw message (~400 KB)
//! is dropped — the app shows an oversized-message warning for it — while the compressed
//! message (~5x smaller, ~80 KB) fits and renders smoothly. Run with `--points 6000` to
//! fit the raw cloud under the cap and compare the two topics side by side instead. The
//! startup log prints both sizes and their deliverability verdicts.
//!
//! `--method` and `--quantization-bits` control the Draco settings for `/cloud/compressed`,
//! for eyeballing the quality/size trade-offs: fewer bits shrink the message but show
//! visible quantization stepping in the wave, and sequential encoding preserves point
//! order at a larger size.
//!
//! Requires `FOXGLOVE_DEVICE_TOKEN` (and `FOXGLOVE_API_URL` for a local platform stack).
//!
//! To view: connect to the device in the Foxglove app, add a 3D panel, enable
//! `/cloud/compressed` under the panel's Topics settings (topics are not enabled
//! automatically), and set the display frame to `world`. Color the points by the
//! `intensity` field for best effect. The oversized `/cloud/raw` drops surface as a
//! warning in the app's problems list.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Parser;
use foxglove::draco::{
    CompressPointCloudOptions, DracoEncodeOptions, DracoMethod, MAX_QUANTIZATION_BITS,
};
use foxglove::messages::{
    FrameTransform, PackedElementField, PointCloud, Quaternion, Timestamp, Vector3,
    packed_element_field::NumericType,
};
use foxglove::{ChannelDescriptor, Encode, LazyChannel, remote_access::Gateway};

/// The default per-message size limit for lossy remote access data, in bytes. Messages
/// larger than this are dropped before publishing.
const DATA_TRACK_MESSAGE_CAP: usize = 100 * 1024;

/// How often to report message sizes relative to the cap.
const SIZE_REPORT_INTERVAL: Duration = Duration::from_secs(5);

static COMPRESSED_CHANNEL: LazyChannel<PointCloud> = LazyChannel::new("/cloud/compressed");
static RAW_CHANNEL: LazyChannel<PointCloud> = LazyChannel::new("/cloud/raw");
static TF_CHANNEL: LazyChannel<FrameTransform> = LazyChannel::new("/tf");

#[derive(Parser)]
struct Args {
    /// Approximate number of points in the cloud. The default exceeds the raw data-track
    /// cap while the compressed cloud still fits; use 6000 or less to deliver the raw
    /// topic too.
    #[arg(long, default_value_t = 25_000)]
    points: usize,

    /// Frames per second.
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..))]
    fps: u32,

    /// Draco encoding method for /cloud/compressed. kd-tree compresses better but
    /// reorders points; sequential preserves point order.
    #[arg(long, value_enum, default_value = "kd-tree")]
    method: Method,

    /// Quantization bits for positions on /cloud/compressed. 0 encodes positions as
    /// lossless float32 (larger output, and falls back to sequential encoding).
    #[arg(long, default_value_t = 12,
          value_parser = clap::value_parser!(u8).range(0..=MAX_QUANTIZATION_BITS as i64))]
    quantization_bits: u8,
}

/// Draco encoding method (mirrors [`DracoMethod`] for clap).
#[derive(Clone, Copy, clap::ValueEnum)]
enum Method {
    KdTree,
    Sequential,
}

impl From<Method> for DracoMethod {
    fn from(method: Method) -> Self {
        match method {
            Method::KdTree => Self::KdTree,
            Method::Sequential => Self::Sequential,
        }
    }
}

/// A wave surface animated over time, with an intensity field.
///
/// Points are a `side x side` grid in the xy plane; z carries a travelling wave. Each
/// point is 16 bytes: x/y/z/intensity float32.
fn make_cloud(side: usize, t: f64) -> PointCloud {
    const POINT_STRIDE: usize = 16;
    let extent = 10.0_f64;
    let mut data = Vec::with_capacity(side * side * POINT_STRIDE);
    for iy in 0..side {
        for ix in 0..side {
            let x = (ix as f64 / side as f64 - 0.5) * extent;
            let y = (iy as f64 / side as f64 - 0.5) * extent;
            let z = ((x + t).sin() * (y + t * 0.7).cos()) * 1.5;
            let intensity = ((z / 1.5) as f32 + 1.0) / 2.0;
            data.extend_from_slice(&(x as f32).to_le_bytes());
            data.extend_from_slice(&(y as f32).to_le_bytes());
            data.extend_from_slice(&(z as f32).to_le_bytes());
            data.extend_from_slice(&intensity.to_le_bytes());
        }
    }

    let field = |name: &str, offset: u32| PackedElementField {
        name: name.to_string(),
        offset,
        r#type: NumericType::Float32 as i32,
    };
    PointCloud {
        timestamp: Some(now()),
        frame_id: "cloud".to_string(),
        pose: None,
        point_stride: POINT_STRIDE as u32,
        fields: vec![
            field("x", 0),
            field("y", 4),
            field("z", 8),
            field("intensity", 12),
        ],
        data: data.into(),
    }
}

fn now() -> Timestamp {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch");
    Timestamp::new(epoch.as_secs() as u32, epoch.subsec_nanos())
}

/// A static identity transform parenting the cloud frame to `world`. Timestamp and
/// rotation are required fields.
fn world_transform() -> FrameTransform {
    FrameTransform {
        timestamp: Some(now()),
        parent_frame_id: "world".to_string(),
        child_frame_id: "cloud".to_string(),
        translation: Some(Vector3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }),
        rotation: Some(Quaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }),
    }
}

/// Returns the encoded protobuf size of a message, which is what the data-track cap is
/// measured against.
fn encoded_size(msg: &impl Encode) -> usize {
    let mut buf = Vec::new();
    msg.encode(&mut buf).expect("encoding is infallible");
    buf.len()
}

/// Reports the raw and compressed message sizes against the data-track cap.
fn report_sizes(cloud: &PointCloud, options: &DracoEncodeOptions) {
    let verdict = |size: usize| {
        if size > DATA_TRACK_MESSAGE_CAP {
            "WILL BE DROPPED"
        } else {
            "deliverable"
        }
    };

    let raw_size = encoded_size(cloud);
    tracing::info!(
        "/cloud/raw message: {raw_size} bytes (cap {DATA_TRACK_MESSAGE_CAP}) — {}",
        verdict(raw_size)
    );

    // Preview what the sink's background transcoder will produce for the compressed
    // topic, using the same settings configured on the gateway.
    match foxglove::draco::compress_point_cloud(cloud, options) {
        Ok(compressed) => {
            let compressed_size = encoded_size(&compressed);
            tracing::info!(
                "/cloud/compressed message: {compressed_size} bytes ({:.1}x smaller) — {}",
                raw_size as f64 / compressed_size as f64,
                verdict(compressed_size)
            );
        }
        Err(e) => tracing::warn!("compression preview failed: {e}"),
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let side = (args.points as f64).sqrt().round() as usize;
    tracing::info!(
        "streaming a {side}x{side} ({} point) wave at {} fps on /cloud/compressed \
         (draco) and /cloud/raw (compression suppressed)",
        side * side,
        args.fps
    );

    let options = DracoEncodeOptions {
        method: args.method.into(),
        quantization_bits: args.quantization_bits,
    };

    // Report the deliverability verdicts up front, before connecting.
    report_sizes(&make_cloud(side, 0.0), &options);

    // Compression applies to every compressible channel; the raw topic opts out so the
    // two delivery paths can be compared. The handle is held for the life of the
    // process; the connection runs until exit.
    let _gateway = Gateway::new()
        .compress_point_clouds(Some(CompressPointCloudOptions::Draco(options)))
        .suppress_point_cloud_compression_fn(|channel: &ChannelDescriptor| {
            channel.topic() == "/cloud/raw"
        })
        .start()
        .expect("failed to start gateway");

    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / f64::from(args.fps)));
    let mut last_report = SystemTime::now();
    let start = SystemTime::now();
    loop {
        ticker.tick().await;
        let t = SystemTime::now()
            .duration_since(start)
            .unwrap_or_default()
            .as_secs_f64();

        let cloud = make_cloud(side, t);
        if last_report.elapsed().unwrap_or_default() >= SIZE_REPORT_INTERVAL {
            report_sizes(&cloud, &options);
            last_report = SystemTime::now();
        }

        TF_CHANNEL.log(&world_transform());
        COMPRESSED_CHANNEL.log(&cloud);
        RAW_CHANNEL.log(&cloud);
    }
}
