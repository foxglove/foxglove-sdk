use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::websocket::Status;

use super::{ConnectedClient, ShutdownReason};

/// A poller for a connected client.
///
/// The poller is repsonsible for:
/// - Sending messages (from `data_plane` and `control_plane`) to the websocket.
/// - Receiving messages from the websocket and invoking [`ConnectedClient::handle_message`].
/// - Waiting for a shutdown signal, and closing the websocket.
pub(super) struct Poller {
    websocket: WebSocketStream<TcpStream>,
    data_plane_rx: flume::Receiver<Message>,
    control_plane_rx: flume::Receiver<Message>,
    shutdown_rx: oneshot::Receiver<ShutdownReason>,
}

impl Poller {
    /// Creates a new poller.
    pub fn new(
        websocket: WebSocketStream<TcpStream>,
        data_plane_rx: flume::Receiver<Message>,
        control_plane_rx: flume::Receiver<Message>,
        shutdown_rx: oneshot::Receiver<ShutdownReason>,
    ) -> Self {
        Self {
            websocket,
            data_plane_rx,
            control_plane_rx,
            shutdown_rx,
        }
    }

    /// Runs the main poll loop for a websocket connection.
    pub async fn run(self, client: &ConnectedClient) {
        let addr = client.addr();
        let (mut ws_tx, mut ws_rx) = self.websocket.split();

        // Handle messages received from the websocket.
        let ws_rx_loop = async {
            while let Some(msg) = ws_rx.next().await {
                match msg {
                    Ok(Message::Close(_)) => break,
                    Ok(msg) => client.handle_message(msg),
                    Err(err) => tracing::error!("Error receiving from client {addr}: {err}"),
                }
            }
            tracing::info!("Connection closed by client {addr}");
            ShutdownReason::ClientDisconnected
        };

        // Send messages from queues to the websocket.
        let ws_tx_loop = async {
            while let Ok(msg) = tokio::select! {
                msg = self.control_plane_rx.recv_async() => msg,
                msg = self.data_plane_rx.recv_async() => msg,
            } {
                if let Err(err) = ws_tx.send(msg).await {
                    tracing::error!("Error sending message to client {addr}: {err}");
                }
            }
            unreachable!("ConnectedClient holds queues");
        };

        // Run send and receive loops concurrently.
        let reason = tokio::select! {
            _ = ws_tx_loop => unreachable!("ConnectedClient holds queues"),
            r = ws_rx_loop => r,
            r = self.shutdown_rx => r.expect("ConnectedClient sends before dropping sender"),
        };

        // Send final messages, as appropriate.
        match reason {
            ShutdownReason::ClientDisconnected => (),
            ShutdownReason::ServerStopped => {
                ws_tx.send(Message::Close(None)).await.ok();
            }
            ShutdownReason::ControlPlaneQueueFull => {
                let status = Status::error(
                    "Disconnected because the message backlog on the server is full. \
                    The backlog size is configurable in the server setup.",
                );
                ws_tx.send(Message::from(&status)).await.ok();
                ws_tx.send(Message::Close(None)).await.ok();
            }
        }
    }
}
