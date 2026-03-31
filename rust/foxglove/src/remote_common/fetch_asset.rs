//! Fetch asset handler and responder types.

use std::any::Any;
use std::fmt::Display;

use crate::remote_common::semaphore::SemaphoreGuard;

/// A transport-agnostic sender for fetch asset responses.
pub(crate) trait ResponseSender: Send {
    /// Sends a fetch asset response.
    ///
    /// `result` is either `Ok(data)` for a successful response,
    /// or `Err(message)` for an error response.
    fn send(&mut self, result: Result<&[u8], String>);
}

/// A handler to respond to fetch asset requests.
///
/// This can be used to serve assets to the Foxglove app, including URDF files for the 3D panel.
pub trait AssetHandler: Send + Sync + 'static {
    /// Fetch an asset with the given uri and return it via the responder.
    ///
    /// This method is invoked from the client's main poll loop and must not block. If blocking or
    /// long-running behavior is required, the implementation should use [`tokio::task::spawn`]
    /// or [`tokio::task::spawn_blocking`] to handle the request asynchronously.
    fn fetch(&self, uri: String, responder: AssetResponder);
}

/// A handle for completing a fetch asset request.
///
/// If you're holding one of these, you're responsible for eventually calling
/// [`AssetResponder::respond`], [`AssetResponder::respond_ok`], or [`AssetResponder::respond_err`].
/// If you drop the responder without responding, the client will receive a generic error.
#[must_use]
pub struct AssetResponder {
    /// Transport-specific context set by the transport layer and extracted by
    /// transport-specific handler adapters (e.g. `BlockingAssetHandlerFn`).
    context: Option<Box<dyn Any + Send>>,
    inner: Option<Inner>,
}

impl std::fmt::Debug for AssetResponder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetResponder").finish_non_exhaustive()
    }
}

impl AssetResponder {
    /// Create a new asset responder for a fetch asset request.
    pub(crate) fn new(sender: Box<dyn ResponseSender>, guard: SemaphoreGuard) -> Self {
        Self {
            context: None,
            inner: Some(Inner {
                sender,
                _guard: guard,
            }),
        }
    }

    /// Attaches transport-specific context to this responder.
    pub(crate) fn with_context(mut self, ctx: Box<dyn Any + Send>) -> Self {
        self.context = Some(ctx);
        self
    }

    /// Extracts the transport-specific context, if it is of type `T`.
    pub(crate) fn take_context<T: 'static>(&mut self) -> Option<T> {
        let ctx = self.context.take()?;
        match ctx.downcast::<T>() {
            Ok(val) => Some(*val),
            Err(ctx) => {
                self.context = Some(ctx);
                None
            }
        }
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
        if let Some(mut inner) = self.inner.take() {
            inner.sender.send(Ok(data.as_ref()));
        }
    }

    /// Send an error response to the client.
    pub fn respond_err(mut self, message: impl Into<String>) {
        if let Some(mut inner) = self.inner.take() {
            inner.sender.send(Err(message.into()));
        }
    }
}

impl Drop for AssetResponder {
    fn drop(&mut self) {
        if let Some(mut inner) = self.inner.take() {
            // The asset handler has dropped its responder without responding. This could be due to
            // a panic or some other flaw in implementation. Reply with a generic error message.
            inner.sender.send(Err(
                "Internal server error: asset handler failed to send a response".into(),
            ));
        }
    }
}

struct Inner {
    sender: Box<dyn ResponseSender>,
    _guard: SemaphoreGuard,
}
