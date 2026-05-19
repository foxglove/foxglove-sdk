//! Shared parameter handling primitives.

use crate::protocol::common::parameter::Parameter;
use crate::remote_common::AnyClient;
use crate::remote_common::semaphore::SemaphoreGuard;

/// Internal trait implemented by each transport's `Client` type so that [`AnyClient`] can
/// dispatch parameter responses without exposing the per-transport surface.
pub(crate) trait SendParameterResponse {
    /// Send a `ParameterValues` message to the requesting client.
    fn send_parameter_values(&self, parameters: Vec<Parameter>, request_id: Option<String>);

    /// Broadcast updated parameter values to all clients subscribed to those parameters on the
    /// transport that owns this client.
    fn broadcast_parameter_values(&self, parameters: Vec<Parameter>);
}

/// Handler for client-initiated parameter operations.
///
/// These methods are invoked from time-sensitive contexts and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
///
/// # Note on unset parameter values
///
/// Per the protocol spec, a [`Parameter`] with `value: None` represents an unset/deleted
/// parameter and is not transmitted to clients. Such entries are filtered out of any response
/// or broadcast emitted by the responders below.
pub trait ParameterHandler: Send + Sync + 'static {
    /// Handle a client request to get parameter values.
    ///
    /// `names` is the requested parameter names, or empty to request all parameters. Take
    /// ownership of `responder` and eventually call [`GetParametersResponder::respond`].
    /// Dropping the responder without responding sends a generic error status to the client.
    fn get(
        &self,
        client: AnyClient,
        names: Vec<String>,
        request_id: Option<String>,
        responder: GetParametersResponder,
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
        client: AnyClient,
        parameters: Vec<Parameter>,
        request_id: Option<String>,
        responder: SetParametersResponder,
    );
}

/// Responder for a client `getParameters` request.
///
/// Take ownership and call [`Self::respond`] when the requested parameter values are available.
/// Dropping the responder without responding sends a generic error status to the client.
#[must_use]
#[derive(Debug)]
pub struct GetParametersResponder {
    client: AnyClient,
    inner: Option<ResponderInner>,
}

impl GetParametersResponder {
    pub(crate) fn new(
        client: AnyClient,
        request_id: Option<String>,
        guard: SemaphoreGuard,
    ) -> Self {
        Self {
            client,
            inner: Some(ResponderInner {
                request_id,
                _guard: guard,
            }),
        }
    }

    /// Returns a clone of the requesting client handle.
    pub fn client(&self) -> AnyClient {
        self.client.clone()
    }

    /// Send parameter values back to the requesting client.
    ///
    /// Entries with `value: None` are dropped before serialization (see the note on the
    /// [`ParameterHandler`] trait).
    pub fn respond(mut self, parameters: Vec<Parameter>) {
        if let Some(inner) = self.inner.take() {
            self.client
                .send_parameter_values(parameters, inner.request_id);
        }
    }
}

impl Drop for GetParametersResponder {
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
pub struct SetParametersResponder {
    client: AnyClient,
    inner: Option<ResponderInner>,
}

impl SetParametersResponder {
    pub(crate) fn new(
        client: AnyClient,
        request_id: Option<String>,
        guard: SemaphoreGuard,
    ) -> Self {
        Self {
            client,
            inner: Some(ResponderInner {
                request_id,
                _guard: guard,
            }),
        }
    }

    /// Returns a clone of the requesting client handle.
    pub fn client(&self) -> AnyClient {
        self.client.clone()
    }

    /// Acknowledge the set request with the values that were actually applied. Echoes to the
    /// requester when the request carried a `request_id`, and broadcasts to subscribers.
    ///
    /// Entries with `value: None` are dropped before serialization (see the note on the
    /// [`ParameterHandler`] trait).
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

impl Drop for SetParametersResponder {
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
