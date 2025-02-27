use std::{future::Future, sync::Arc};

use bytes::Bytes;

use super::{Client, SemaphoreGuard};

/// The result of a fetch asset request.
pub type FetchAssetResult = Result<Bytes, String>;

/// A handler to respond to fetch asset requests.
///
/// See: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset>
pub trait AssetHandler: Send + Sync + 'static {
    /// Fetch an asset with the given uri and return it via the responder.
    /// Fetch should not block, it should call `runtime.spawn`
    /// or `runtime.spawn_blocking` to do the actual work.
    fn fetch(self: Arc<Self>, _uri: String, _responder: AssetResponder);
}

pub(crate) struct BlockingAssetHandlerFn<F>(pub F);

impl<F> AssetHandler for BlockingAssetHandlerFn<F>
where
    F: Fn(Client, String) -> FetchAssetResult + Send + Sync + 'static,
{
    fn fetch(self: Arc<Self>, uri: String, responder: AssetResponder) {
        tokio::task::spawn_blocking(move || {
            let result = (self.0)(responder.client(), uri);
            responder.respond(result);
        });
    }
}

pub(crate) struct AsyncAssetHandlerFn<F>(pub F);

impl<F, Fut> AssetHandler for AsyncAssetHandlerFn<F>
where
    F: Fn(Client, String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = FetchAssetResult> + Send + 'static,
{
    fn fetch(self: Arc<Self>, uri: String, responder: AssetResponder) {
        tokio::spawn(async move {
            let result = (self.0)(responder.client(), uri).await;
            responder.respond(result);
        });
    }
}

/// Wraps a weak reference to a Client and provides a method
/// to respond to the fetch asset request from that client.
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
    pub fn respond(self, result: FetchAssetResult) {
        self.client.send_asset_response(result, self.request_id);
    }
}
