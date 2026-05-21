//! Shared parameter handling primitives.

use crate::protocol::common::parameter::Parameter;
use crate::remote_common::AnyClient;
use crate::remote_common::semaphore::SemaphoreGuard;

/// Internal trait implemented by each transport's `Client` type so that [`AnyClient`] can
/// dispatch parameter responses without exposing the per-transport surface.
pub(crate) trait SendParameterResponse {
    /// Send a `ParameterValues` message to the requesting client.
    fn send_parameter_values(&self, parameters: Vec<Parameter>, request_id: Option<String>);
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
/// emitted by the responders below.
pub trait ParameterHandler: Send + Sync + 'static {
    /// Handle a client request to get parameter values.
    ///
    /// `names` is the requested parameter names, or empty to request all parameters. The
    /// implementation **must** take ownership of `responder` and eventually call
    /// [`GetParametersResponder::respond`] (pass an empty vec if no values are available).
    /// Dropping the responder without responding is reserved for unrecoverable internal errors
    /// and sends a generic error status to the client.
    fn get(
        &self,
        client: AnyClient,
        names: Vec<String>,
        request_id: Option<String>,
        responder: GetParametersResponder,
    );

    /// Handle a client request to set parameter values.
    ///
    /// The implementation **must** take ownership of `responder` and eventually call
    /// [`SetParametersResponder::respond`] with the parameters that were actually applied, even
    /// if the request could not be handled (pass an empty vec in that case). Dropping the
    /// responder without responding is reserved for unrecoverable internal errors and sends a
    /// generic error status to the client.
    ///
    /// When `request_id` is present, the parameters passed to
    /// [`SetParametersResponder::respond`] are echoed back to the requesting client. The
    /// responder does not notify other parameter subscribers: a handler may be shared across
    /// multiple sinks, so it is the implementer's responsibility to broadcast updates to
    /// subscribers on each sink (for example, via
    /// [`WebSocketServerHandle::publish_parameter_values`](crate::WebSocketServerHandle::publish_parameter_values)
    /// or
    /// [`GatewayHandle::publish_parameter_values`](crate::remote_access::GatewayHandle::publish_parameter_values)).
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
/// The handler **must** call [`Self::respond`] with the requested parameter values (pass an
/// empty vec if none are available). Dropping the responder without responding is reserved for
/// unrecoverable internal errors and sends a generic error status to the requesting client.
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
/// The handler **must** call [`Self::respond`] with the parameters that were actually applied,
/// even when the request could not be handled (pass an empty vec in that case). Dropping the
/// responder without responding is reserved for unrecoverable internal errors and sends a
/// generic error status to the requesting client.
///
/// When the request carried a `request_id`, the values passed to [`Self::respond`] are echoed
/// back to the requester; otherwise the responder does nothing on the wire.
///
/// # Differences from the legacy listener
///
/// The responder does not notify other parameter subscribers. This is a behavior change from
/// the `ServerListener::on_set_parameters` / `Listener::on_set_parameters` path, which
/// publishes the returned values to subscribers automatically. See [`ParameterHandler::set`]
/// for the rationale and how to broadcast updates from a handler implementation.
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
    /// requester when the request carried a `request_id`, otherwise does nothing.
    ///
    /// Entries with `value: None` are dropped before serialization (see the note on the
    /// [`ParameterHandler`] trait).
    pub fn respond(mut self, parameters: Vec<Parameter>) {
        if let Some(inner) = self.inner.take() {
            if inner.request_id.is_some() {
                self.client
                    .send_parameter_values(parameters, inner.request_id);
            }
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
