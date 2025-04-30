use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;

use super::semaphore::SemaphoreGuard;
use super::Client;

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

impl<F, T, Err> AssetHandler for BlockingAssetHandlerFn<F>
where
    F: Fn(Client, String) -> Result<T, Err> + Send + Sync + 'static,
    T: AsRef<[u8]>,
    Err: Display,
{
    fn fetch(&self, uri: String, responder: AssetResponder) {
        let func = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let result = (func)(responder.client(), uri);
            responder.respond(result);
        });
    }
}

pub(crate) struct AsyncAssetHandlerFn<F>(pub Arc<F>);

impl<F, Fut, T, Err> AssetHandler for AsyncAssetHandlerFn<F>
where
    F: Fn(Client, String) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, Err>> + Send + 'static,
    T: AsRef<[u8]>,
    Err: Display,
{
    fn fetch(&self, uri: String, responder: AssetResponder) {
        let func = self.0.clone();
        tokio::spawn(async move {
            let result = (func)(responder.client(), uri).await;
            responder.respond(result);
        });
    }
}

/// Wraps a weak reference to a Client and provides a method
/// to respond to the fetch asset request from that client.
#[must_use]
#[derive(Debug)]
pub struct AssetResponder {
    client: Client,
    inner: Option<AssetResponderInner>,
}

impl AssetResponder {
    /// Create a new asset responder for a fetch asset request.
    pub(crate) fn new(client: Client, request_id: u32, guard: SemaphoreGuard) -> Self {
        Self {
            client,
            inner: Some(AssetResponderInner {
                request_id,
                _guard: guard,
            }),
        }
    }

    /// Return a clone of the Client.
    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Send a result to the client.
    pub fn respond<T, Err>(self, result: Result<T, Err>)
    where
        T: AsRef<[u8]>,
        Err: Display,
    {
        match result {
            Ok(data) => self.respond_ok(data.as_ref()),
            Err(e) => self.respond_err(e.to_string()),
        }
    }

    /// Send response data to the client.
    pub fn respond_ok(mut self, data: impl AsRef<[u8]>) {
        if let Some(inner) = self.inner.take() {
            inner.respond(&self.client, Ok(data.as_ref()))
        }
    }

    /// Send an error response to the client.
    pub fn respond_err(mut self, message: impl AsRef<str>) {
        if let Some(inner) = self.inner.take() {
            inner.respond(&self.client, Err(message.as_ref()))
        }
    }
}

impl Drop for AssetResponder {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            // The asset handler has dropped its responder without responding. This could be due to
            // a panic or some other flaw in implementation. Reply with a generic error message.
            inner.respond(
                &self.client,
                Err("Internal server error: asset handler failed to send a response"),
            )
        }
    }
}

#[derive(Debug)]
struct AssetResponderInner {
    request_id: u32,
    _guard: SemaphoreGuard,
}

impl AssetResponderInner {
    /// Send an response to the client.
    fn respond(self, client: &Client, result: Result<&[u8], &str>) {
        client.send_asset_response(result, self.request_id);
    }
}
