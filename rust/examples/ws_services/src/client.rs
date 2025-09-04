use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::types::{IntBinRequest, IntBinResponse, SetBoolRequest, SetBoolResponse};
use crate::Config;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use protocol::{
    AdvertiseServices, ServerMessage, ServiceCallFailure, ServiceCallRequest, ServiceCallResponse,
    UnadvertiseServices, SDK_SUBPROTOCOL,
};
use serde_json::json;
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, JoinSet};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};
use tracing::{error, info};

mod protocol;

pub async fn main(config: Config) -> Result<()> {
    let client = Arc::new(Client::connect(&config.host, config.port).await?);

    // Make some calls.
    let sum = client.call_add(4, 38).await?;
    info!("got sum: {sum}");

    let echo = client.call_echo("echo! echo!".into()).await?;
    info!("got echo: {echo}");

    let result = client.call_set_flag(true).await?;
    info!("set flag: {result:?}");

    let result = client.call_set_flag(false).await?;
    info!("clear flag: {result:?}");

    // Launch a bunch of concurrent calls, expecting some of them to fail with "Too many requests".
    let mut sleepers = JoinSet::default();
    for id in 0..50 {
        let client = client.clone();
        sleepers.spawn(async move {
            let result = if id % 2 == 0 {
                client.call_sleep().await
            } else {
                client.call_blocking().await
            };
            if let Err(e) = result {
                error!("{id} failed to sleep: {e}");
            } else {
                info!("{id} is awake");
            }
        });
    }
    let _ = sleepers.join_all().await;

    client.close().await
}

type Stream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Client {
    tx: tokio::sync::Mutex<SplitSink<Stream, Message>>,
    state: State,
    rx_task: JoinHandle<()>,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.rx_task.abort();
    }
}
impl Client {
    /// Connects to a websocket server and validates the subprotocol.
    async fn connect(host: &str, port: u16) -> Result<Self> {
        let mut request = format!("ws://{host}:{port}/")
            .into_client_request()
            .context("Failed to build request")?;

        request
            .headers_mut()
            .insert("sec-websocket-protocol", SDK_SUBPROTOCOL);

        let (stream, response) = tokio_tungstenite::connect_async(request)
            .await
            .context("Failed to connect")?;

        let Some(protocol) = response.headers().get("sec-websocket-protocol") else {
            return Err(anyhow!("Missing subprotocol from server"));
        };

        if protocol != SDK_SUBPROTOCOL {
            return Err(anyhow!("Unexpected subprotocol from server: {protocol:?}"));
        }

        let (tx, rx) = stream.split();
        let state = State::default();
        let rx_task = tokio::spawn(rx_task(rx, state.clone()));
        Ok(Self {
            tx: tokio::sync::Mutex::new(tx),
            state,
            rx_task,
        })
    }

    /// Sends a message.
    async fn send(&self, msg: Message) -> Result<()> {
        let mut tx = self.tx.lock().await;
        tx.send(msg).await.context("failed to send message")?;
        Ok(())
    }

    /// Makes a service call.
    async fn service_call(
        &self,
        service_name: &str,
        encoding: &str,
        payload: Bytes,
    ) -> Result<Bytes> {
        loop {
            let Some(service_id) = self.state.get_service_id(service_name) else {
                info!("Waiting for service {service_name} to be advertised");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            let call = self.state.service_call(service_id, encoding, payload);
            self.send(call.msg).await?;
            return call.rx.await.context("failed to get response")?;
        }
    }

    async fn call_add(&self, a: u64, b: u64) -> Result<u64> {
        let req = serde_json::to_vec(&IntBinRequest { a, b })?;
        let resp = self
            .service_call("/IntBin/add", "json", req.into())
            .await
            .context("failed to call /IntBin/add")?;
        let resp: IntBinResponse =
            serde_json::from_slice(&resp).context("failed to parse /IntBin/add response")?;
        Ok(resp.result)
    }

    async fn call_echo(&self, msg: String) -> Result<String> {
        let req = json!({ "msg": msg }).to_string().into();
        let resp = self
            .service_call("/echo", "json", req)
            .await
            .context("failed to call /echo")?;
        let resp = String::from_utf8(resp.to_vec()).context("invalid echo response")?;
        Ok(resp)
    }

    async fn call_sleep(&self) -> Result<()> {
        self.service_call("/sleep", "json", Bytes::new())
            .await
            .context("failed to call /sleep")?;
        Ok(())
    }

    async fn call_blocking(&self) -> Result<()> {
        self.service_call("/blocking", "json", Bytes::new())
            .await
            .context("failed to call /blocking")?;
        Ok(())
    }

    async fn call_set_flag(&self, data: bool) -> Result<SetBoolResponse> {
        let req = serde_json::to_vec(&SetBoolRequest { data })?;
        let resp = self
            .service_call("/flag_a", "json", req.into())
            .await
            .context("failed to call /flag_a")?;
        let resp: SetBoolResponse = serde_json::from_slice(&resp)?;
        Ok(resp)
    }

    /// Gracefully closes the websocket.
    async fn close(&self) -> Result<()> {
        let mut tx = self.tx.lock().await;
        tx.close().await?;
        Ok(())
    }
}

/// Main poll loop for receiving messages from the websocket server.
async fn rx_task(mut rx: SplitStream<Stream>, state: State) {
    while let Some(msg) = rx.next().await {
        let msg = msg.expect("Failed to receive message");
        let msg = match ServerMessage::parse_message(msg) {
            Ok(msg) => msg,
            Err(err) => {
                error!("Failed to parse message: {err}");
                continue;
            }
        };
        let Some(msg) = msg else {
            continue;
        };
        match msg {
            ServerMessage::ServerInfo => (),
            ServerMessage::AdvertiseServices(msg) => state.on_advertise_services(msg),
            ServerMessage::UnadvertiseServices(msg) => state.on_unadvertise_services(msg),
            ServerMessage::ServiceCallResponse(msg) => state.on_service_call_response(msg),
            ServerMessage::ServiceCallFailure(msg) => state.on_service_call_failure(msg),
        }
    }
}

#[derive(Default)]
struct Inner {
    /// Map service names to service IDs.
    services: HashMap<String, u32>,
    /// Map of service call IDs to response channels.
    service_calls: HashMap<u32, oneshot::Sender<Result<Bytes>>>,
}

/// An outstanding service call.
struct ServiceCall {
    /// The encoded message to send.
    msg: Message,
    /// A channel on which to receive the response.
    rx: oneshot::Receiver<Result<Bytes>>,
}

#[derive(Default, Clone)]
struct State(Arc<parking_lot::RwLock<Inner>>);
impl State {
    /// Registers advertised services in the services map.
    fn on_advertise_services(&self, msg: AdvertiseServices) {
        let mut inner = self.0.write();
        for service in msg.services {
            let name = service.name;
            let id = service.id;
            if let Some(prev_id) = inner.services.insert(name.clone(), id) {
                info!("Updated service {} id ({} -> {})", name, prev_id, id);
            } else {
                info!("Added service {} ({})", name, id);
            }
        }
    }

    /// Unregisters unadvertised services from the services map.
    fn on_unadvertise_services(&self, msg: UnadvertiseServices) {
        let mut inner = self.0.write();
        let ids: HashSet<_> = msg.service_ids.into_iter().collect();
        inner.services.retain(|name, id| {
            if ids.contains(id) {
                info!("Removed service {} ({})", name, id);
                false
            } else {
                true
            }
        });
    }

    /// Handles a service call response.
    fn on_service_call_response(&self, msg: ServiceCallResponse) {
        self.complete_service_call(msg.call_id, Ok(msg.payload));
    }

    /// Handles a service call failure.
    fn on_service_call_failure(&self, msg: ServiceCallFailure) {
        self.complete_service_call(msg.call_id, Err(anyhow!(msg.message)));
    }

    /// Looks up a service by name.
    fn get_service_id(&self, name: &str) -> Option<u32> {
        self.0.read().services.get(name).copied()
    }

    /// Prepares a new service call by encoding the message and registering a response channel.
    fn service_call(&self, service_id: u32, encoding: &str, payload: Bytes) -> ServiceCall {
        static CALL_ID: AtomicU32 = AtomicU32::new(0);
        let call_id = CALL_ID.fetch_add(1, Ordering::Relaxed);
        let msg = ServiceCallRequest {
            service_id,
            call_id,
            encoding,
            payload,
        }
        .encode();
        let (tx, rx) = oneshot::channel();
        let prev = self.0.write().service_calls.insert(call_id, tx);
        assert!(prev.is_none());
        ServiceCall { msg, rx }
    }

    /// Completes a service call by sending the result on the response channel.
    fn complete_service_call(&self, call_id: u32, result: Result<Bytes>) {
        let mut inner = self.0.write();
        if let Some(tx) = inner.service_calls.remove(&call_id) {
            let _ = tx.send(result);
        } else {
            error!("unexpected callback for {call_id}");
        }
    }
}
