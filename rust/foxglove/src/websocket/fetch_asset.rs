use super::Client;
use crate::remote_common::fetch_asset;

pub use fetch_asset::AssetHandler;

/// Type alias for the websocket-specific asset responder.
pub type AssetResponder = fetch_asset::AssetResponder<Client>;

pub(crate) use fetch_asset::{AsyncAssetHandlerFn, BlockingAssetHandlerFn};
