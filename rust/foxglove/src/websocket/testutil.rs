use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_native_tls::native_tls;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::handshake::SUBPROTOCOL;
use super::ws_protocol::server::ServerMessage;
use super::ws_protocol::ParseError;

#[derive(Debug, thiserror::Error)]
pub enum RecvError {
    #[error("unexpected end of stream")]
    UnexpectedEndOfStream,
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error(transparent)]
    Tungstenite(#[from] tungstenite::Error),
    #[error(transparent)]
    Timeout(#[from] tokio::time::error::Elapsed),
}

pub struct WebSocketClient {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl WebSocketClient {
    /// Connects to a server and validates the handshake response.
    pub async fn connect(addr: impl std::fmt::Display) -> Self {
        Self::do_connect(addr, false).await
    }

    pub async fn connect_secure(addr: impl std::fmt::Display) -> Self {
        Self::do_connect(addr, true).await
    }

    pub async fn do_connect(addr: impl std::fmt::Display, use_tls: bool) -> Self {
        let protocol = if use_tls { "wss" } else { "ws" };
        let mut request = format!("{protocol}://{addr}/")
            .into_client_request()
            .expect("Failed to build request");

        request.headers_mut().insert(
            "sec-websocket-protocol",
            HeaderValue::from_static(SUBPROTOCOL),
        );

        let (stream, response) = if use_tls {
            // For tests, ignore TLS errors related to self-signed certs
            let connector = native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .expect("Failed to build TLS connector");

            let connector = tokio_tungstenite::Connector::NativeTls(connector);

            tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
                .await
                .expect("Failed to connect (TLS)")
        } else {
            tokio_tungstenite::connect_async(request)
                .await
                .expect("Failed to connect")
        };

        assert_eq!(
            response.headers().get("sec-websocket-protocol"),
            Some(&HeaderValue::from_static(SUBPROTOCOL))
        );

        Self { stream }
    }

    /// Receives a message from the server.
    pub async fn recv_msg(&mut self) -> Result<Message, RecvError> {
        match self.stream.next().await {
            Some(r) => r.map_err(RecvError::from),
            None => Err(RecvError::UnexpectedEndOfStream),
        }
    }

    /// Receives and parses a message from the server.
    pub async fn recv(&mut self) -> Result<ServerMessage, RecvError> {
        let msg = tokio::time::timeout(Duration::from_secs(1), self.recv_msg()).await??;
        let msg = ServerMessage::try_from(&msg)?;
        Ok(msg.into_owned())
    }

    /// Sends a message to the server.
    pub async fn send(&mut self, msg: impl Into<Message>) -> Result<(), tungstenite::Error> {
        self.stream.send(msg.into()).await
    }

    /// Closes the websocket connection.
    pub async fn close(&mut self) {
        self.stream.close(None).await.unwrap();
    }
}
