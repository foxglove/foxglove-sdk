//! Shared parameter handling primitives.

use crate::protocol::common::parameter::Parameter;
use crate::remote_common::semaphore::SemaphoreGuard;

/// Internal trait for sending parameter responses to a client (or broadcast).
pub trait SendParameterResponse: Clone + Send + 'static {
    /// Send a `ParameterValues` message to the requesting client.
    fn send_parameter_values(&self, parameters: Vec<Parameter>, request_id: Option<String>);

    /// Broadcast updated parameter values to all clients subscribed to those parameters on the
    /// transport that owns this client.
    fn broadcast_parameter_values(&self, parameters: Vec<Parameter>);

    /// Send a generic error status to the requesting client. Used by the responder Drop fallback
    /// when a handler fails to respond.
    fn send_error(&self, message: &str);
}

/// Handler for client-initiated parameter operations.
///
/// These methods are invoked from time-sensitive contexts and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
pub trait ParameterHandler<C: SendParameterResponse>: Send + Sync + 'static {
    /// Handle a client request to get parameter values.
    ///
    /// `names` is the requested parameter names, or empty to request all parameters. Take
    /// ownership of `responder` and eventually call [`GetParametersResponder::respond`].
    /// Dropping the responder without responding sends a generic error status to the client.
    fn get(
        &self,
        client: C,
        names: Vec<String>,
        request_id: Option<String>,
        responder: GetParametersResponder<C>,
    );

    /// Handle a client request to set parameter values.
    ///
    /// Take ownership of `responder` and eventually call [`SetParametersResponder::respond`]
    /// with the parameters that were actually updated. Those values are echoed back to the
    /// requesting client when `request_id` is present, and broadcast to all clients subscribed
    /// to those parameter names. Dropping the responder without responding sends a generic
    /// error status to the client and does *not* broadcast anything.
    fn set(
        &self,
        client: C,
        parameters: Vec<Parameter>,
        request_id: Option<String>,
        responder: SetParametersResponder<C>,
    );

    /// Invoked when a parameter name acquires its first subscriber.
    ///
    /// Default implementation is a no-op; override if you need to track which parameters have
    /// active subscribers (e.g. to subscribe to upstream parameter-update events).
    fn subscribe(&self, _names: Vec<String>) {}

    /// Invoked when a parameter name loses its last subscriber (or its last subscriber
    /// disconnects).
    ///
    /// Default implementation is a no-op; override symmetrically with [`Self::subscribe`].
    fn unsubscribe(&self, _names: Vec<String>) {}
}

/// Responder for a client `getParameters` request.
///
/// Take ownership and call [`Self::respond`] when the requested parameter values are available.
/// Dropping the responder without responding sends a generic error status to the client.
#[must_use]
#[derive(Debug)]
pub struct GetParametersResponder<C: SendParameterResponse> {
    client: C,
    inner: Option<ResponderInner>,
}

impl<C: SendParameterResponse> GetParametersResponder<C> {
    pub(crate) fn new(client: C, request_id: Option<String>, guard: SemaphoreGuard) -> Self {
        Self {
            client,
            inner: Some(ResponderInner {
                request_id,
                _guard: guard,
            }),
        }
    }

    /// Returns a clone of the requesting client handle.
    pub fn client(&self) -> C {
        self.client.clone()
    }

    /// Send parameter values back to the requesting client.
    pub fn respond(mut self, parameters: Vec<Parameter>) {
        if let Some(inner) = self.inner.take() {
            self.client
                .send_parameter_values(parameters, inner.request_id);
        }
    }
}

impl<C: SendParameterResponse> Drop for GetParametersResponder<C> {
    fn drop(&mut self) {
        if self.inner.take().is_some() {
            self.client
                .send_error("Internal server error: parameter handler failed to send a response");
        }
    }
}

/// Responder for a client `setParameters` request.
///
/// Take ownership and call [`Self::respond`] with the parameters that were actually applied. The
/// responder echoes those values to the requesting client (when the request carried a
/// `request_id`) and broadcasts them to all clients subscribed to those parameter names.
///
/// Dropping the responder without responding sends an error status to the requesting client and
/// does not broadcast anything.
#[must_use]
#[derive(Debug)]
pub struct SetParametersResponder<C: SendParameterResponse> {
    client: C,
    inner: Option<ResponderInner>,
}

impl<C: SendParameterResponse> SetParametersResponder<C> {
    pub(crate) fn new(client: C, request_id: Option<String>, guard: SemaphoreGuard) -> Self {
        Self {
            client,
            inner: Some(ResponderInner {
                request_id,
                _guard: guard,
            }),
        }
    }

    /// Returns a clone of the requesting client handle.
    pub fn client(&self) -> C {
        self.client.clone()
    }

    /// Acknowledge the set request with the values that were actually applied. Echoes to the
    /// requester when the request carried a `request_id`, and broadcasts to subscribers.
    pub fn respond(mut self, parameters: Vec<Parameter>) {
        if let Some(inner) = self.inner.take() {
            if inner.request_id.is_some() {
                self.client
                    .send_parameter_values(parameters.clone(), inner.request_id);
            }
            self.client.broadcast_parameter_values(parameters);
        }
    }
}

impl<C: SendParameterResponse> Drop for SetParametersResponder<C> {
    fn drop(&mut self) {
        if self.inner.take().is_some() {
            self.client
                .send_error("Internal server error: parameter handler failed to send a response");
        }
    }
}

#[derive(Debug)]
struct ResponderInner {
    request_id: Option<String>,
    /// Held to release a slot on the per-client parameter semaphore when the responder is
    /// consumed or dropped.
    _guard: SemaphoreGuard,
}
