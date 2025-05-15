//! Service call response handling.

use std::borrow::Cow;
use std::fmt::Display;
use std::sync::Arc;

use tokio_tungstenite::tungstenite::Message;

use super::{CallId, ServiceId};
use crate::websocket::connected_client::ConnectedClient;
use crate::websocket::semaphore::SemaphoreGuard;
use crate::websocket::ws_protocol::server::{ServiceCallFailure, ServiceCallResponse};

/// A handle for completing a service call.
///
/// If you're holding one of these, you're responsible for eventually calling
/// [`Responder::respond`]. If you drop the responder without responding, the client will never
/// receive a response for its request.
#[must_use]
#[derive(Debug)]
pub struct Responder(Option<Inner>);
impl Responder {
    /// Creates a new responder.
    pub(in crate::websocket) fn new(
        client: Arc<ConnectedClient>,
        service_id: ServiceId,
        call_id: CallId,
        encoding: impl Into<String>,
        _guard: SemaphoreGuard,
    ) -> Self {
        Self(Some(Inner {
            client,
            service_id,
            call_id,
            encoding: encoding.into(),
            _guard,
        }))
    }

    /// Overrides the default response encoding.
    ///
    /// By default, the response encoding is the one declared in the
    /// [`ServiceSchema`][super::ServiceSchema]. If no response encoding was declared, then the
    /// encoding is presumed to be the same as the request.
    pub fn set_encoding(&mut self, encoding: impl Into<String>) {
        if let Some(inner) = self.0.as_mut() {
            inner.encoding = encoding.into();
        }
    }

    /// Send a result to the client.
    pub fn respond<T, E>(self, result: Result<T, E>)
    where
        T: AsRef<[u8]>,
        E: Display,
    {
        match result {
            Ok(data) => self.respond_ok(data),
            Err(e) => self.respond_err(e.to_string()),
        }
    }

    /// Send response data to the client.
    pub fn respond_ok(mut self, data: impl AsRef<[u8]>) {
        if let Some(inner) = self.0.take() {
            inner.respond(Ok(data.as_ref()))
        }
    }

    /// Send an error response to the client.
    pub fn respond_err(mut self, message: String) {
        if let Some(inner) = self.0.take() {
            inner.respond(Err(message))
        }
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if let Some(inner) = self.0.take() {
            // The service call handler has dropped its responder without responding. This could be
            // due to a panic or some other flaw in implementation. Reply with a generic error
            // message.
            inner.respond(Err(
                "Internal server error: service failed to send a response".into(),
            ))
        }
    }
}

#[derive(Debug)]
struct Inner {
    client: Arc<ConnectedClient>,
    service_id: ServiceId,
    call_id: CallId,
    encoding: String,
    _guard: SemaphoreGuard,
}

impl Inner {
    fn respond(self, result: Result<&[u8], String>) {
        let message = match result {
            Ok(payload) => Message::from(&ServiceCallResponse {
                service_id: self.service_id.into(),
                call_id: self.call_id.into(),
                encoding: self.encoding.into(),
                payload: Cow::Borrowed(payload),
            }),
            Err(message) => Message::from(&ServiceCallFailure {
                service_id: self.service_id.into(),
                call_id: self.call_id.into(),
                message,
            }),
        };

        // Callee logs errors.
        let _ = self.client.send_control_msg(message);
    }
}
