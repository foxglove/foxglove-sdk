use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use foxglove::schemas::{RawImage, Timestamp};
use foxglove::LazyChannel;
use opencv::{
    core::Mat,
    prelude::*,
    videoio::{VideoCapture, CAP_ANY},
};

/// RGB Camera visualization with Foxglove
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Camera ID (0 for default camera, or path to video file)
    #[arg(short, long, default_value = "0")]
    camera_id: String,
}

// Create a channel for publishing camera images
static IMAGE_CHANNEL: LazyChannel<RawImage> = LazyChannel::new("/camera/image");

struct CameraCapture {
    cap: VideoCapture,
    camera_id: String,
}

impl CameraCapture {
    fn new(camera_id: String) -> Result<Self> {
        let cap = if let Ok(id) = camera_id.parse::<i32>() {
            // Camera ID (integer)
            VideoCapture::new(id, CAP_ANY)?
        } else {
            // Video file path
            VideoCapture::from_file(&camera_id, CAP_ANY)?
        };

        if !cap.is_opened()? {
            anyhow::bail!("Camera {} is not opened", camera_id);
        }

        let width = cap.get(opencv::videoio::CAP_PROP_FRAME_WIDTH)?;
        let height = cap.get(opencv::videoio::CAP_PROP_FRAME_HEIGHT)?;
        let fps = cap.get(opencv::videoio::CAP_PROP_FPS)?;

        println!("Camera connected successfully:");
        println!("  ID/Path: {}", camera_id);
        println!("  Resolution: {} x {}", width as i32, height as i32);
        println!("  Frame Rate: {:.1} fps", fps);

        Ok(Self { cap, camera_id })
    }

    fn read_frame(&mut self) -> Result<Option<Mat>> {
        let mut frame = Mat::default();

        if !self.cap.read(&mut frame)? {
            return Ok(None);
        }

        if frame.empty() {
            return Ok(None);
        }

        Ok(Some(frame))
    }
}

fn create_raw_image_message(frame: &Mat) -> Result<RawImage> {
    let height = frame.rows() as u32;
    let width = frame.cols() as u32;
    let channels = frame.channels() as u32;

    let data = frame.data_bytes()?.to_vec();

    let foxglove_timestamp = Timestamp::now();

    Ok(RawImage {
        timestamp: Some(foxglove_timestamp),
        frame_id: "camera".to_string(),
        width,
        height,
        encoding: "bgr8".to_string(), // OpenCV default is BGR
        step: width * channels,       // bytes per row
        data: data.into(),
    })
}

fn camera_loop(mut camera: CameraCapture, done: Arc<AtomicBool>) -> Result<()> {
    while !done.load(Ordering::Relaxed) {
        match camera.read_frame() {
            Ok(Some(frame)) => match create_raw_image_message(&frame) {
                Ok(img_msg) => {
                    IMAGE_CHANNEL.log(&img_msg);
                }
                Err(e) => {
                    eprintln!("Failed to create image message: {}", e);
                }
            },
            Ok(None) => {
                eprintln!("Failed to read frame from camera");
            }
            Err(e) => {
                eprintln!("Error reading frame: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Args::parse();

    let done = Arc::new(AtomicBool::new(false));

    // Set up Ctrl+C handler
    ctrlc::set_handler({
        let done = done.clone();
        move || {
            println!("\nShutting down camera visualization...");
            done.store(true, Ordering::Relaxed);
        }
    })
    .context("Failed to set SIGINT handler")?;

    let server = foxglove::WebSocketServer::new()
        .start_blocking()
        .context("Failed to start Foxglove server")?;
    println!("Foxglove server started at {}", server.app_url());

    let camera = CameraCapture::new(args.camera_id).context("Failed to initialize camera")?;
    println!(
        "Starting camera {} feed... Press Ctrl+C to stop.",
        camera.camera_id
    );

    if let Err(e) = camera_loop(camera, done.clone()) {
        eprintln!("Camera loop error: {}", e);
        return Err(e);
    }

    println!("Camera visualization stopped.");
    Ok(())
}
