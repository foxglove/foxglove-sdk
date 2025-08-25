//! Facades for TLS support when the "tls" feature is disabled.

use crate::{
    websocket::streams::{Acceptor, ServerStream, TlsIdentity},
    FoxgloveError,
};
use tokio_util::either::Either;

pub(crate) type TlsStream<S> = S;

pub struct StreamConfiguration {}

impl StreamConfiguration {
    /// Returns an error if a TlsIdentity is provided.
    pub fn new(identity: Option<&TlsIdentity>) -> Result<Self, FoxgloveError> {
        if identity.is_some() {
            return Err(FoxgloveError::ConfigurationError(
                "TLS is not enabled".to_string(),
            ));
        }

        Ok(Self {})
    }
}

impl Acceptor for StreamConfiguration {
    /// Always returns a plain stream.
    async fn accept(
        &self,
        stream: tokio::net::TcpStream,
    ) -> Result<ServerStream<tokio::net::TcpStream>, crate::FoxgloveError> {
        Ok(Either::Left(stream))
    }

    fn accepts_tls(&self) -> bool {
        false
    }
}
