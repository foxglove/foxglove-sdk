use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "tls")]
use rcgen::Certificate;
use std::sync::Arc;
use tokio::net::TcpStream;
#[cfg(feature = "tls")]
use tokio_rustls::rustls;
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
        Self::do_connect(addr, None).await
    }

    #[cfg(feature = "tls")]
    pub async fn connect_secure(addr: impl std::fmt::Display, trusted_cert: Certificate) -> Self {
        Self::do_connect(addr, Some(trusted_cert)).await
    }

    pub async fn do_connect(addr: impl std::fmt::Display, tls_cert: Option<Certificate>) -> Self {
        let use_tls = tls_cert.is_some();
        let protocol = if use_tls { "wss" } else { "ws" };
        let mut request = format!("{protocol}://{addr}/")
            .into_client_request()
            .expect("Failed to build request");

        request.headers_mut().insert(
            "sec-websocket-protocol",
            HeaderValue::from_static(SUBPROTOCOL),
        );

        let (stream, response) = if let Some(tls_cert) = tls_cert {
            #[cfg(feature = "tls")]
            {
                let mut root_cert_store = rustls::RootCertStore::empty();
                root_cert_store
                    .add(tls_cert.der().clone().into_owned())
                    .expect("failed to add cert to root cert store");
                let config = rustls::ClientConfig::builder()
                    .with_root_certificates(root_cert_store)
                    .with_no_client_auth();

                let connector = tokio_tungstenite::Connector::Rustls(Arc::new(config));

                tokio_tungstenite::connect_async_tls_with_config(
                    request,
                    None,
                    false,
                    Some(connector),
                )
                .await
                .expect("Failed to connect (TLS)")
            }
            #[cfg(not(feature = "tls"))]
            {
                unimplemented!()
            }
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
