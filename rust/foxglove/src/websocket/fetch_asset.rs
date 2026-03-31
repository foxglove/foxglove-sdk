//! Websocket fetch asset handler.

use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;

use super::client::Client;
use crate::remote_common::fetch_asset::ResponseSender;
use crate::remote_common::semaphore::SemaphoreGuard;
use crate::websocket::connected_client::ConnectedClient;

// Re-export common types.
pub use crate::remote_common::fetch_asset::{AssetHandler, AssetResponder};

/// A wrapper around a blocking function that serves as a fetch asset handler.
///
/// The handler function receives the requesting [`Client`] and the asset URI.
pub(crate) struct BlockingAssetHandlerFn<F>(pub Arc<F>);

impl<F, T, Err> AssetHandler for BlockingAssetHandlerFn<F>
where
    F: Fn(Client, String) -> Result<T, Err> + Send + Sync + 'static,
    T: AsRef<[u8]>,
    Err: Display,
{
    fn fetch(&self, uri: String, mut responder: AssetResponder) {
        let client = responder
            .take_context::<Client>()
            .expect("BlockingAssetHandlerFn requires a websocket Client context");
        let func = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let result = (func)(client, uri);
            responder.respond(result);
        });
    }
}

/// A wrapper around an async function that serves as a fetch asset handler.
///
/// The handler function receives the requesting [`Client`] and the asset URI.
pub(crate) struct AsyncAssetHandlerFn<F>(pub Arc<F>);

impl<F, Fut, T, Err> AssetHandler for AsyncAssetHandlerFn<F>
where
    F: Fn(Client, String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, Err>> + Send + 'static,
    T: AsRef<[u8]>,
    Err: Display,
{
    fn fetch(&self, uri: String, mut responder: AssetResponder) {
        let client = responder
            .take_context::<Client>()
            .expect("AsyncAssetHandlerFn requires a websocket Client context");
        let func = self.0.clone();
        tokio::spawn(async move {
            let result = (func)(client, uri).await;
            responder.respond(result);
        });
    }
}

/// Sends fetch asset responses over a WebSocket connection.
struct WsResponseSender {
    client: Arc<ConnectedClient>,
    request_id: u32,
}

impl ResponseSender for WsResponseSender {
    fn send(&mut self, result: Result<&[u8], String>) {
        match result {
            Ok(data) => self.client.send_asset_response(data, self.request_id),
            Err(message) => self.client.send_asset_error(&message, self.request_id),
        }
    }
}

/// Creates a new [`AssetResponder`] backed by a WebSocket connection.
pub(in crate::websocket) fn new_responder(
    client: Arc<ConnectedClient>,
    request_id: u32,
    guard: SemaphoreGuard,
) -> AssetResponder {
    let ws_client = Client::new(&client);
    let sender = Box::new(WsResponseSender { client, request_id });
    AssetResponder::new(sender, guard).with_context(Box::new(ws_client))
}
