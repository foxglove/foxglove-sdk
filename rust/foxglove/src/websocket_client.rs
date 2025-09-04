use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "tls")]
use rcgen::Certificate;
use tokio::net::TcpStream;
#[cfg(feature = "tls")]
use tokio_rustls::rustls;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::websocket::handshake::SUBPROTOCOL;

use super::ws_protocol::server::ServerMessage;
use super::ws_protocol::ParseError;

#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
pub enum WebSocketClientError {
    #[error("unexpected end of stream")]
    UnexpectedEndOfStream,
    #[error("invalid subprotocol")]
    InvalidSubprotocol,
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error(transparent)]
    Tungstenite(#[from] tungstenite::Error),
    #[error(transparent)]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[cfg(feature = "tls")]
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
}

#[doc(hidden)]
pub struct WebSocketClient {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WebSocketClient {
    /// Connects to a server and validates the handshake response.
    #[cfg(feature = "tls")]
    pub async fn connect_secure(
        addr: impl AsRef<str>,
        trusted_cert: Certificate,
    ) -> Result<Self, WebSocketClientError> {
        let mut request = format!("wss://{addr}/", addr = addr.as_ref())
            .into_client_request()
            .expect("Failed to build request");

        request.headers_mut().insert(
            "sec-websocket-protocol",
            HeaderValue::from_static(SUBPROTOCOL),
        );

        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store
            .add(trusted_cert.der().clone().into_owned())
            .map_err(WebSocketClientError::from)?;
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();

        let connector = tokio_tungstenite::Connector::Rustls(std::sync::Arc::new(config));

        let (stream, response) =
            tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
                .await
                .map_err(WebSocketClientError::from)?;

        if response.headers().get("sec-websocket-protocol")
            != Some(&HeaderValue::from_static(SUBPROTOCOL))
        {
            return Err(WebSocketClientError::InvalidSubprotocol);
        }

        Ok(Self { stream })
    }

    pub async fn connect(addr: impl AsRef<str>) -> Result<Self, WebSocketClientError> {
        let mut request = format!("ws://{addr}/", addr = addr.as_ref())
            .into_client_request()
            .expect("Failed to build request");

        request.headers_mut().insert(
            "sec-websocket-protocol",
            HeaderValue::from_static(SUBPROTOCOL),
        );

        let (stream, response) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(WebSocketClientError::from)?;

        if response.headers().get("sec-websocket-protocol")
            != Some(&HeaderValue::from_static(SUBPROTOCOL))
        {
            return Err(WebSocketClientError::InvalidSubprotocol);
        }

        Ok(Self { stream })
    }

    /// Receives a message from the server.
    pub async fn recv_msg(&mut self) -> Result<Message, WebSocketClientError> {
        match self.stream.next().await {
            Some(r) => r.map_err(WebSocketClientError::from),
            None => Err(WebSocketClientError::UnexpectedEndOfStream),
        }
    }

    /// Receives and parses a message from the server.
    pub async fn recv(&mut self) -> Result<ServerMessage<'_>, WebSocketClientError> {
        let msg = tokio::time::timeout(Duration::from_secs(1), self.recv_msg()).await??;
        let msg = ServerMessage::try_from(&msg)?;
        Ok(msg.into_owned())
    }

    /// Sends a message to the server.
    pub async fn send(&mut self, msg: impl Into<Message>) -> Result<(), WebSocketClientError> {
        self.stream
            .send(msg.into())
            .await
            .map_err(WebSocketClientError::from)
    }

    /// Closes the websocket connection.
    pub async fn close(&mut self) -> Result<(), WebSocketClientError> {
        self.stream
            .close(None)
            .await
            .map_err(WebSocketClientError::from)
    }
}
