//! `MaybeTlsStream` from tokio-tungstenite, adapted for server-side streams in rustls.
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// A stream that might be protected with TLS.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ServerStream<S> {
    /// Unencrypted socket stream.
    Plain(S),
    /// Encrypted socket stream using `rustls`.
    #[cfg(feature = "tls")]
    Tls(tokio_rustls::server::TlsStream<S>),
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for ServerStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            ServerStream::Plain(ref mut s) => Pin::new(s).poll_read(cx, buf),
            #[cfg(feature = "tls")]
            ServerStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for ServerStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            ServerStream::Plain(ref mut s) => Pin::new(s).poll_write(cx, buf),
            #[cfg(feature = "tls")]
            ServerStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            ServerStream::Plain(ref mut s) => Pin::new(s).poll_flush(cx),
            #[cfg(feature = "tls")]
            ServerStream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            ServerStream::Plain(ref mut s) => Pin::new(s).poll_shutdown(cx),
            #[cfg(feature = "tls")]
            ServerStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}
