use bytes::Bytes;
use clap::Parser;

use foxglove::websocket::{AssetHandler, AssetResponder};
use std::collections::HashMap;

struct AssetServer {
    assets: HashMap<String, Bytes>,
}

impl AssetServer {
    fn new() -> Self {
        let mut assets = HashMap::new();
        assets.insert("/test/one".to_string(), Bytes::from_static(b"one"));
        assets.insert("/test/two".to_string(), Bytes::from_static(b"two"));

        Self { assets }
    }
}

impl AssetHandler for AssetServer {
    fn fetch(&self, uri: String, responder: AssetResponder) {
        match self.assets.get(&uri) {
            // A real implementation might use std::fs::read to read a file into a Vec<u8>
            // The ws-protocol doesn't currently support streaming for a single asset.
            Some(asset) => responder.respond(Ok(asset.clone())),
            None => responder.respond(Err(format!("Asset {} not found", uri))),
        }
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
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    let asset_server = AssetServer::new();

    let server = foxglove::WebSocketServer::new()
        .name(env!("CARGO_PKG_NAME"))
        .bind(&args.host, args.port)
        .fetch_asset_handler(Box::new(asset_server))
        .start()
        .await
        .expect("Server failed to start");

    tokio::signal::ctrl_c().await.ok();
    server.stop().await;
}
