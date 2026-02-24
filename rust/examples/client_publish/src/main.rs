//! Example websocket server with client publish
//!
//! This example uses the 'unstable' feature to expose capabilities.
//!
//! Usage:
//! ```text
//! cargo run -p example_client_publish
//! ```

use clap::Parser;
use foxglove::convert::SaturatingInto;
use foxglove::schemas::log::Level;
use foxglove::schemas::Log;
use foxglove::websocket::{Capability, Client, ClientChannel, ServerListener};
use foxglove::{Channel, WebSocketServer};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(short, long, default_value_t = 8765)]
    port: u16,
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

struct ExampleCallbackHandler;
impl ServerListener for ExampleCallbackHandler {
    fn on_message_data(&self, client: Client, channel: &ClientChannel, message: &[u8]) {
        let json: serde_json::Value =
            serde_json::from_slice(message).expect("Failed to parse message");
        println!(
            "Client {} published to channel {}: {json}",
            client.id(),
            channel.id
        );
    }

    fn on_client_advertise(&self, client: Client, channel: &ClientChannel) {
        println!(
            "Client {} advertised channel: {}",
            client.id(),
            channel.topic
        );
    }

    fn on_client_unadvertise(&self, client: Client, channel: &ClientChannel) {
        println!(
            "Client {} unadvertised channel: {}",
            client.id(),
            channel.topic
        );
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    let server = WebSocketServer::new()
        .name(env!("CARGO_PKG_NAME"))
        .bind(args.host, args.port)
        .capabilities([Capability::ClientPublish])
        .listener(Arc::new(ExampleCallbackHandler))
        .supported_encodings(["json"])
        .start()
        .await
        .expect("Failed to start server");

    let shutdown = watch_ctrl_c();
    tokio::select! {
        () = shutdown.cancelled() => (),
        () = log_forever() => (),
    };

    server.stop().wait().await;
}

async fn log_forever() {
    let channel = Channel::new("/log");
    let start = Instant::now();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        let msg = Log {
            timestamp: Some(SystemTime::now().saturating_into()),
            message: format!("It's been {:?}", start.elapsed()),
            level: Level::Info.into(),
            ..Default::default()
        };
        channel.log(&msg);
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
