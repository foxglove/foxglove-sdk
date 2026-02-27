//! A websocket server without a #[tokio::main] entrypoint.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use clap::Parser;

use foxglove::LazyChannel;

#[derive(Debug, foxglove::Encode)]
struct Message {
    msg: String,
    count: u32,
}

static MSG_CHANNEL: LazyChannel<Message> = LazyChannel::new("/msg");

fn log_until(fps: u8, stop: Arc<AtomicBool>) {
    let mut count: u32 = 0;
    let duration = Duration::from_millis(1000 / u64::from(fps));
    while !stop.load(Ordering::Relaxed) {
        MSG_CHANNEL.log(&Message {
            msg: "Hello, world!".to_string(),
            count,
        });
        std::thread::sleep(duration);
        count += 1;
    }
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

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    let done = Arc::new(AtomicBool::default());
    ctrlc::set_handler({
        let done = done.clone();
        move || {
            done.store(true, Ordering::Relaxed);
        }
    })
    .expect("Failed to set SIGINT handler");

    let server = foxglove::WebSocketServer::new()
        .name(env!("CARGO_PKG_NAME"))
        .bind(&args.host, args.port)
        .start_blocking()
        .expect("Server failed to start");

    log_until(args.fps, done);

    server.stop().wait_blocking();
}
