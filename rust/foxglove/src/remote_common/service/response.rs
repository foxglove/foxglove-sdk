//! Service call response handling.

use std::fmt::Display;

/// A transport-agnostic sender for service call responses.
pub(crate) trait ResponseSender: Send {
    /// Sends a service call response.
    ///
    /// `result` is either `Ok((encoding, payload))` for a successful response,
    /// or `Err(message)` for a failure response.
    fn send(&mut self, result: Result<(&str, &[u8]), String>);
}

/// A handle for completing a service call.
///
/// If you're holding one of these, you're responsible for eventually calling
/// [`Responder::respond`]. If you drop the responder without responding, the client will never
/// receive a response for its request.
#[must_use]
pub struct Responder(Option<Inner>);

impl std::fmt::Debug for Responder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Responder").finish_non_exhaustive()
    }
}

impl Responder {
    /// Creates a new responder backed by the given `ResponseSender`.
    pub(crate) fn new(encoding: impl Into<String>, sender: Box<dyn ResponseSender>) -> Self {
        Self(Some(Inner {
            encoding: encoding.into(),
            sender,
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
        if let Some(mut inner) = self.0.take() {
            inner.sender.send(Ok((&inner.encoding, data.as_ref())));
        }
    }

    /// Send an error response to the client.
    pub fn respond_err(mut self, message: String) {
        if let Some(mut inner) = self.0.take() {
            inner.sender.send(Err(message));
        }
    }
}

impl Drop for Responder {
    fn drop(&mut self) {
        if let Some(mut inner) = self.0.take() {
            // The service call handler has dropped its responder without responding. This could be
            // due to a panic or some other flaw in implementation. Reply with a generic error
            // message.
            inner.sender.send(Err(
                "Internal server error: service failed to send a response".into(),
            ));
        }
    }
}

struct Inner {
    encoding: String,
    sender: Box<dyn ResponseSender>,
}
