use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;

use crate::websocket::service::{Request, Responder};

/// A websocket service call handler.
pub trait Handler: Send + Sync {
    /// Handles a service call request from a client.
    ///
    /// The caller can choose whether to handle the call synchronously or asynchronously.
    ///
    /// This method is invoked from the client's main poll loop and must not block. If blocking or
    /// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
    /// [`tokio::task::spawn_blocking`]) to handle the request asynchronously.
    ///
    /// The implementation is responsible for completing the request with [`Responder::respond`],
    /// otherwise no response will be sent to the client.
    fn call(&self, request: Request, responder: Responder);
}

/// A synchronous service call handler.
///
/// This is a convenience wrapper around [`Handler`] that takes care of moving the result into the
/// [`Responder`], so that the implementation can take advantage of standard control flow idioms
/// for returning errors.
pub trait SyncHandler: Send + Sync {
    /// The error type returned for service calls.
    type Error: Display;

    /// Response payload type.
    type Response: AsRef<[u8]> + 'static;

    /// Synchronously handles a service call request from a client and returns a result.
    ///
    /// This method is invoked from the client's main poll loop and must not block. If blocking or
    /// long-running behavior is required, use [`Handler`] instead.
    fn call(&self, request: Request) -> Result<Self::Response, Self::Error>;
}

impl<T: SyncHandler> Handler for T {
    fn call(&self, request: Request, responder: Responder) {
        let result = SyncHandler::call(self, request);
        responder.respond(result);
    }
}

/// A wrapper around a function that serves as a service call handler.
pub(crate) struct HandlerFn<F, T, E>(pub F)
where
    F: Fn(Request) -> Result<T, E> + Send + Sync,
    T: AsRef<[u8]> + 'static,
    E: Display;

impl<F, T, E> SyncHandler for HandlerFn<F, T, E>
where
    F: Fn(Request) -> Result<T, E> + Send + Sync,
    T: AsRef<[u8]> + 'static,
    E: Display,
{
    type Error = E;
    type Response = T;

    fn call(&self, request: Request) -> Result<Self::Response, Self::Error> {
        self.0(request)
    }
}

/// A wrapper around a blocking function that serves as a service call handler.
pub(crate) struct BlockingHandlerFn<F, T, E>(pub Arc<F>)
where
    F: Fn(Request) -> Result<T, E> + Send + Sync + 'static,
    T: AsRef<[u8]> + 'static,
    E: Display;

impl<F, T, E> Handler for BlockingHandlerFn<F, T, E>
where
    F: Fn(Request) -> Result<T, E> + Send + Sync + 'static,
    T: AsRef<[u8]> + 'static,
    E: Display,
{
    fn call(&self, request: Request, responder: Responder) {
        let func = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let result = (func)(request);
            responder.respond(result);
        });
    }
}

/// A wrapper around a async function that serves as a service call handler.
pub(crate) struct AsyncHandlerFn<F, Fut, T, E>(pub Arc<F>)
where
    F: Fn(Request) -> Fut + Send + Sync,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: AsRef<[u8]> + 'static,
    E: Display;

impl<F, Fut, T, E> Handler for AsyncHandlerFn<F, Fut, T, E>
where
    F: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, E>> + Send,
    T: AsRef<[u8]> + 'static,
    E: Display + Send,
{
    fn call(&self, request: Request, responder: Responder) {
        let func = self.0.clone();
        tokio::task::spawn(async move {
            let result = (func)(request).await;
            responder.respond(result);
        });
    }
}
