use foxglove::{
    ChannelDescriptor,
    bytes::Bytes,
    remote_access::{Capability, Client, Gateway, Listener},
    schemas::{CameraCalibration, RawImage, Timestamp},
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

struct MessageHandler;
impl Listener for MessageHandler {
    /// Called when a connected app publishes a message, such as from the Teleop panel.
    fn on_message_data(&self, client: Client, channel: &ChannelDescriptor, message: &[u8]) {
        let json = serde_json::from_slice::<Value>(message).expect("Failed to parse message");
        println!(
            "Teleop message from {} on topic {}: {json}",
            client.id(),
            channel.topic()
        );
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    // Open a gateway for remote visualization and teleop.
    let handle = Gateway::new()
        .capabilities([Capability::ClientPublish])
        .supported_encodings(["json"])
        .listener(Arc::new(MessageHandler))
        .start()
        .expect("Failed to start remote access gateway");

    tokio::task::spawn(camera_loop());
    _ = tokio::signal::ctrl_c().await;
    _ = handle.stop().await;
}

/// Convert a hue in [0, 360) to an (R, G, B) tuple with full saturation and value.
fn hue_to_rgb(h: f64) -> (u8, u8, u8) {
    let sector = h / 60.0;
    let x = 1.0 - (sector % 2.0 - 1.0).abs();
    let (r, g, b) = match sector as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Log RawImage messages, which will be encoded as a video stream when sent to the remote access gateway.
async fn camera_loop() {
    let mut interval = tokio::time::interval(Duration::from_millis(1000 / 30));
    let width: usize = 960;
    let height: usize = 540;

    // Pre-compute a double-width gradient lookup table so we can take a
    // width-sized slice at any offset without per-pixel math each frame.
    let mono_gradient: Vec<u8> = (0..width * 2)
        .map(|x| ((x % width) * 255 / width) as u8)
        .collect();

    // Pre-compute a double-width rainbow lookup table (3 bytes per pixel).
    let rgb_gradient: Vec<u8> = (0..width * 2)
        .flat_map(|x| {
            let hue = (x % width) as f64 * 360.0 / width as f64;
            let (r, g, b) = hue_to_rgb(hue);
            [r, g, b]
        })
        .collect();

    let calibration = CameraCalibration {
        timestamp: None,
        frame_id: "".into(),
        width: width as u32,
        height: height as u32,
        distortion_model: String::new(),
        d: vec![],
        k: vec![
            500.0,
            0.0,
            width as f64 / 2.0,
            0.0,
            500.0,
            height as f64 / 2.0,
            0.0,
            0.0,
            1.0,
        ],
        r: vec![],
        p: vec![
            500.0,
            0.0,
            width as f64 / 2.0,
            0.0,
            0.0,
            500.0,
            height as f64 / 2.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ],
    };

    let mut offset = 0usize;

    loop {
        interval.tick().await;

        // Mono image: all rows are identical; just repeat the shifted gradient row.
        let mono_row = &mono_gradient[offset..offset + width];
        let mono_img = RawImage {
            timestamp: Some(Timestamp::now()),
            frame_id: "".into(),
            width: width as u32,
            height: height as u32,
            encoding: "mono8".into(),
            step: width as u32,
            data: Bytes::from(mono_row.repeat(height)),
        };
        foxglove::log!("/camera/mono", mono_img);

        // RGB rainbow image: same approach with 3 bytes per pixel.
        let rgb_row = &rgb_gradient[offset * 3..(offset + width) * 3];
        let rgb_img = RawImage {
            timestamp: Some(Timestamp::now()),
            frame_id: "".into(),
            width: width as u32,
            height: height as u32,
            encoding: "rgb8".into(),
            step: (width * 3) as u32,
            data: Bytes::from(rgb_row.repeat(height)),
        };
        foxglove::log!("/camera/rgb", rgb_img);

        foxglove::log!("/camera/info", calibration.clone());

        offset = (offset + 1) % width;
    }
}
