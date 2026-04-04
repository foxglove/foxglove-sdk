//! Remote access gateway example: demonstrates serving assets via the fetch
//! asset handler and logging scene updates that reference those assets.
//!
//! The pelican STL model is embedded at compile time and served when a client
//! requests `package://pelican/pelican.stl`. A `SceneUpdate` referencing the
//! model is logged every second on the `/scene` topic.
//!
//! Set the `FOXGLOVE_DEVICE_TOKEN` environment variable before running:
//!
//! ```text
//! FOXGLOVE_DEVICE_TOKEN=<your-token> cargo run -p example_remote_access_fetch_asset
//! ```
//!
//! Then open [Foxglove](https://app.foxglove.dev) and connect to the device
//! via the remote access gateway.

use std::collections::HashMap;
use std::time::Duration;

use foxglove::LazyChannel;
use foxglove::messages::{
    Color, ModelPrimitive, Pose, Quaternion, SceneEntity, SceneUpdate, Vector3,
};
use foxglove::remote_access::{AssetHandler, AssetResponder, Gateway};
use log::info;

const PELICAN_URI: &str = "package://pelican/pelican.stl";

struct AssetServer {
    assets: HashMap<String, &'static [u8]>,
}

impl AssetServer {
    fn new() -> Self {
        let mut assets: HashMap<_, &'static [u8]> = HashMap::new();
        assets.insert(
            PELICAN_URI.to_string(),
            include_bytes!("../../pelican.stl").as_slice(),
        );
        Self { assets }
    }
}

impl AssetHandler for AssetServer {
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

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let asset_server = AssetServer::new();

    let handle = Gateway::new()
        .fetch_asset_handler(Box::new(asset_server))
        .start()
        .expect("Failed to start remote access gateway");

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

    _ = handle.stop().await;
}
