use foxglove::{
    bytes::Bytes,
    schemas::RawImage,
    websocket::{Client, ClientChannel},
    CloudSinkListener,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};

struct MessageHandler;
impl CloudSinkListener for MessageHandler {
    /// Called when a connected app publishes a message, such as from the Teleop panel.
    fn on_message_data(&self, client: Client, channel: &ClientChannel, message: &[u8]) {
        let json = serde_json::from_slice::<Value>(message).expect("Failed to parse message");
        println!(
            "Teleop message from {} on topic {}: {json}",
            client.id(),
            channel.topic
        );
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    // Open a connection for remote visualization and teleop.
    // This requires Foxglove Agent to be running on the same machine.
    let handle = foxglove::CloudSink::new()
        .listener(Arc::new(MessageHandler))
        .start()
        .await
        .expect("Failed to start cloud sink");

    tokio::task::spawn(camera_loop());
    _ = tokio::signal::ctrl_c().await;
    if let Some(shutdown) = handle.stop() {
        shutdown.wait().await;
    }
}

/// Log RawImage messages, which will be encoded as a video stream when sent to the Cloud Sink.
async fn camera_loop() {
    let mut interval = tokio::time::interval(Duration::from_millis(1000 / 30));
    let mut offset = 0u32;
    let width = 960;
    let height = 540;

    loop {
        interval.tick().await;

        let data = gradient_data(width, height, offset as usize);
        let img = RawImage {
            width: width as u32,
            height: height as u32,
            encoding: "bgr8".into(),
            step: (width * 3) as u32,
            data: Bytes::from(data),
            ..Default::default()
        };
        foxglove::log!("/camera", img);

        offset = (offset + 1) % width as u32;
    }
}

/// Produce example image data (a gradient). Offset can be used to 'animate' the gradient.
fn gradient_data(width: usize, height: usize, offset: usize) -> Vec<u8> {
    let mut data = vec![0u8; width * height * 3];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 3;
            let shifted_x = (x + offset) % width;
            let gradient = (shifted_x * 255 / width) as u8;

            // B, G, R
            data[idx] = gradient;
            data[idx + 1] = 255 - gradient;
            data[idx + 2] = gradient / 2;
        }
    }
    data
}
