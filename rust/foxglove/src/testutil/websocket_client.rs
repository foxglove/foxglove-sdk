use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "websocket-tls")]
use rcgen::Certificate;
use tokio::net::TcpStream;
#[cfg(feature = "websocket-tls")]
use tokio_rustls::rustls;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::websocket::handshake::{SUBPROTOCOL_V1, SUBPROTOCOL_V2, Subprotocol};

use crate::ws_protocol::ParseError;
use crate::ws_protocol::server::ServerMessage;

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
    #[cfg(feature = "websocket-tls")]
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
}

/// Subprotocols offered by a client that only speaks `foxglove.sdk.v1`.
const OFFER_V1: &str = SUBPROTOCOL_V1;
/// Subprotocols offered by a client that prefers `foxglove.sdk.v2` but accepts `foxglove.sdk.v1`.
const OFFER_V2: &str = "foxglove.sdk.v2, foxglove.sdk.v1";

#[doc(hidden)]
pub struct WebSocketClient {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    subprotocol: Subprotocol,
}

impl WebSocketClient {
    /// Connects to a server and validates the handshake response.
    #[cfg(feature = "websocket-tls")]
    pub async fn connect_secure(
        addr: impl AsRef<str>,
        trusted_cert: Certificate,
    ) -> Result<Self, WebSocketClientError> {
        let mut request = format!("wss://{addr}/", addr = addr.as_ref())
            .into_client_request()
            .expect("Failed to build request");

        request
            .headers_mut()
            .insert("sec-websocket-protocol", HeaderValue::from_static(OFFER_V1));

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

        let subprotocol = Self::negotiated_subprotocol(&response)?;
        Ok(Self {
            stream,
            subprotocol,
        })
    }

    /// Connects to a server, offering only the `foxglove.sdk.v1` subprotocol.
    pub async fn connect(addr: impl AsRef<str>) -> Result<Self, WebSocketClientError> {
        Self::connect_with_offer(addr, OFFER_V1).await
    }

    /// Connects to a server, offering both subprotocols with `foxglove.sdk.v2` preferred.
    pub async fn connect_v2(addr: impl AsRef<str>) -> Result<Self, WebSocketClientError> {
        Self::connect_with_offer(addr, OFFER_V2).await
    }

    async fn connect_with_offer(
        addr: impl AsRef<str>,
        offer: &'static str,
    ) -> Result<Self, WebSocketClientError> {
        let mut request = format!("ws://{addr}/", addr = addr.as_ref())
            .into_client_request()
            .expect("Failed to build request");

        request
            .headers_mut()
            .insert("sec-websocket-protocol", HeaderValue::from_static(offer));

        let (stream, response) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(WebSocketClientError::from)?;

        let subprotocol = Self::negotiated_subprotocol(&response)?;
        Ok(Self {
            stream,
            subprotocol,
        })
    }

    fn negotiated_subprotocol(
        response: &tungstenite::handshake::client::Response,
    ) -> Result<Subprotocol, WebSocketClientError> {
        let header = response.headers().get("sec-websocket-protocol");
        if header == Some(&HeaderValue::from_static(SUBPROTOCOL_V1)) {
            Ok(Subprotocol::V1)
        } else if header == Some(&HeaderValue::from_static(SUBPROTOCOL_V2)) {
            Ok(Subprotocol::V2)
        } else {
            Err(WebSocketClientError::InvalidSubprotocol)
        }
    }

    /// Returns the subprotocol selected by the server.
    pub(crate) fn subprotocol(&self) -> Subprotocol {
        self.subprotocol
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

    /// Closes the WebSocket connection.
    pub async fn close(&mut self) -> Result<(), WebSocketClientError> {
        self.stream
            .close(None)
            .await
            .map_err(WebSocketClientError::from)
    }
}
