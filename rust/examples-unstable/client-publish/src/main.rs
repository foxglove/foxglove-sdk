//! Example websocket server with client publsih
//!
//! This example uses the 'unstable' feature to expose capabilities.
//!
//! Usage:
//! ```text
//! cargo run -p example-client-publish
//! ```

use clap::Parser;
use foxglove::convert::SaturatingInto;
use foxglove::schemas::log::Level;
use foxglove::schemas::Log;
use foxglove::websocket::{Capability, Client, ClientChannel, ServerListener};
use foxglove::{PartialMetadata, TypedChannel, WebSocketServer};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(short, long, default_value_t = 8765)]
    port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value = "example-json-server")]
    server_name: String,
}

struct ExampleCallbackHandler;
impl ServerListener for ExampleCallbackHandler {
    fn on_message_data(&self, _client: Client, channel: &ClientChannel, message: &[u8]) {
        let json: serde_json::Value =
            serde_json::from_slice(message).expect("Failed to parse message");
        println!("Received message on channel {}: {json}", channel.id());
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    let server = WebSocketServer::new()
        .name("client-publish")
        .bind(args.host, args.port)
        .capabilities([Capability::ClientPublish])
        .listener(Arc::new(ExampleCallbackHandler))
        .start()
        .await
        .expect("Failed to start server");

    let shutdown = watch_ctrl_c();
    tokio::select! {
        () = shutdown.cancelled() => (),
        () = log_forever() => (),
    };

    server.stop().await;
}

async fn log_forever() {
    let channel = TypedChannel::new("/log").expect("Failed to create channel");
    let start = Instant::now();
    let mut sequence = 0;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let msg = Log {
            timestamp: Some(SystemTime::now().saturating_into()),
            message: format!("It's been {:?}", start.elapsed()),
            level: Level::Info.into(),
            ..Default::default()
        };
        let meta = PartialMetadata {
            sequence: Some(sequence),
            publish_time: Some(nanoseconds_since_epoch()),
            ..Default::default()
        };
        channel.log_with_meta(&msg, meta);
        sequence += 1;
    }
}

fn watch_ctrl_c() -> CancellationToken {
    let token = CancellationToken::new();
    tokio::spawn({
        let token = token.clone();
        async move {
            tokio::signal::ctrl_c().await.ok();
            token.cancel();
        }
    });
    token
}

fn nanoseconds_since_epoch() -> u64 {
    let now = SystemTime::now();
    if let Ok(duration) = now.duration_since(UNIX_EPOCH) {
        return duration.as_secs() * 1_000_000_000 + duration.subsec_nanos() as u64;
    }
    0
}
