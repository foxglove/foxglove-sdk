//! Support for TLS in the websocket server, if the "tls" feature is enabled.
//! When disabled, provides no-op implementations over plain streams.

#[cfg(not(feature = "tls"))]
mod no_tls;
#[cfg(not(feature = "tls"))]
pub(crate) use no_tls::{StreamConfiguration, TlsStream};

#[cfg(feature = "tls")]
mod rust_tls;
#[cfg(feature = "tls")]
pub(crate) use rust_tls::{StreamConfiguration, TlsStream};
use tokio::net::TcpStream;
use tokio_util::either::Either;

use crate::FoxgloveError;

pub(crate) type ServerStream<S> = Either<S, TlsStream<S>>;

pub(crate) trait Acceptor {
    async fn accept(&self, stream: TcpStream) -> Result<ServerStream<TcpStream>, FoxgloveError>;
    fn accepts_tls(&self) -> bool;
}

/// TLS configuration for a server
#[doc(hidden)]
pub struct TlsIdentity {
    /// A PEM-encoded X.509 certificate.
    pub cert: Vec<u8>,
    /// A PEM-encoded PKCS8 private key.
    pub key: Vec<u8>,
}
