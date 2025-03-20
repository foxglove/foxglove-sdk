use std::{fmt::Display, future::Future, sync::Arc};

use bytes::Bytes;

use super::{Client, SemaphoreGuard};

/// A handler to respond to fetch asset requests.
///
/// See: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset>
pub trait AssetHandler: Send + Sync + 'static {
    /// Fetch an asset with the given uri and return it via the responder.
    /// Fetch should not block, it should call `runtime.spawn`
    /// or `runtime.spawn_blocking` to do the actual work.
    fn fetch(&self, _uri: String, _responder: AssetResponder);
}

pub(crate) struct BlockingAssetHandlerFn<F>(pub Arc<F>);

impl<F, Err> AssetHandler for BlockingAssetHandlerFn<F>
where
    F: Fn(Client, String) -> Result<Bytes, Err> + Send + Sync + 'static,
    Err: Display,
{
    fn fetch(&self, uri: String, responder: AssetResponder) {
        let func = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let result = (func)(responder.client(), uri);
            responder.respond(result.map_err(|e| e.to_string()));
        });
    }
}

pub(crate) struct AsyncAssetHandlerFn<F>(pub Arc<F>);

impl<F, Fut, Err> AssetHandler for AsyncAssetHandlerFn<F>
where
    F: Fn(Client, String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Bytes, Err>> + Send + 'static,
    Err: Display,
{
    fn fetch(&self, uri: String, responder: AssetResponder) {
        let func = self.0.clone();
        tokio::spawn(async move {
            let result = (func)(responder.client(), uri).await;
            responder.respond(result.map_err(|e| e.to_string()));
        });
    }
}

/// Wraps a weak reference to a Client and provides a method
/// to respond to the fetch asset request from that client.
#[derive(Debug)]
pub struct AssetResponder {
    /// The client requesting the asset.
    client: Client,
    request_id: u32,
    _guard: SemaphoreGuard,
}

impl AssetResponder {
    /// Create a new asset responder for a fetch asset request.
    pub(crate) fn new(client: Client, request_id: u32, guard: SemaphoreGuard) -> Self {
        Self {
            client,
            request_id,
            _guard: guard,
        }
    }

    /// Return a clone of the Client.
    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Send an response to the client.
    pub fn respond(self, result: Result<Bytes, String>) {
        self.client.send_asset_response(result, self.request_id);
    }
}
