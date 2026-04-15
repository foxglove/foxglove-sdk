use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use clap::Parser;

use foxglove::LazyChannel;
use foxglove::messages::{
    Color, ModelPrimitive, Pose, Quaternion, SceneEntity, SceneUpdate, Vector3,
};
use foxglove::websocket::{AssetHandler, AssetResponder, Client};
use log::info;

const PELICAN_URI: &str = "package://pelican/pelican.stl";

struct AssetServer {
    assets: HashMap<String, Vec<u8>>,
}

impl AssetServer {
    fn new() -> Self {
        let stl_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../pelican.stl");
        let stl_data = std::fs::read(&stl_path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", stl_path.display()));
        let mut assets = HashMap::new();
        assets.insert(PELICAN_URI.to_string(), stl_data);
        Self { assets }
    }
}

impl AssetHandler<Client> for AssetServer {
    fn fetch(&self, uri: String, responder: AssetResponder) {
        match self.assets.get(&uri) {
            Some(asset) => {
                info!("Serving asset: {uri} ({} bytes)", asset.len());
                responder.respond_ok(asset);
            }
            None => {
                info!("Asset not found: {uri}");
                responder.respond_err(format!("Asset {uri} not found"));
            }
        }
    }
}

static SCENE_CHANNEL: LazyChannel<SceneUpdate> = LazyChannel::new("/scene");

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

    let scene = SceneUpdate {
        deletions: vec![],
        entities: vec![SceneEntity {
            frame_id: "world".to_string(),
            id: "pelican".to_string(),
            models: vec![ModelPrimitive {
                url: PELICAN_URI.to_string(),
                media_type: "model/stl".to_string(),
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
                scale: Some(Vector3 {
                    x: 0.01,
                    y: 0.01,
                    z: 0.01,
                }),
                color: Some(Color {
                    r: 0.8,
                    g: 0.6,
                    b: 0.2,
                    a: 1.0,
                }),
                override_color: false,
                ..Default::default()
            }],
            ..Default::default()
        }],
    };

    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                SCENE_CHANNEL.log(&scene);
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down");
                break;
            }
        }
    }

    server.stop().wait().await;
}
