//! Websocket functionality

use std::collections::hash_map::Entry;
use std::collections::HashSet;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32};
use std::sync::Weak;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use bimap::BiHashMap;
use bytes::Bytes;
use flume::TrySendError;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::{self, handshake::server, http::HeaderValue, Message};
use tokio_tungstenite::WebSocketStream;
use tokio_util::sync::CancellationToken;

use crate::{
    get_runtime_handle, ChannelId, Context, FoxgloveError, Metadata, RawChannel, Sink, SinkId,
};

mod advertise;
mod capability;
mod client_channel;
mod connection_graph;
mod cow_vec;
mod fetch_asset;
mod semaphore;
pub mod service;
mod subscription;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) mod testutil;
pub(crate) mod ws_protocol;

pub use capability::Capability;
pub use client_channel::{ClientChannel, ClientChannelId};
pub use connection_graph::ConnectionGraph;
use cow_vec::CowVec;
pub use fetch_asset::{AssetHandler, AssetResponder};
pub(crate) use fetch_asset::{AsyncAssetHandlerFn, BlockingAssetHandlerFn};
pub(crate) use semaphore::{Semaphore, SemaphoreGuard};
use service::{CallId, Service, ServiceId, ServiceMap};
pub(crate) use subscription::{Subscription, SubscriptionId};
use ws_protocol::client::ClientMessage;
pub use ws_protocol::parameter::{Parameter, ParameterType, ParameterValue};
pub use ws_protocol::server::status::{Level as StatusLevel, Status};
use ws_protocol::server::{
    AdvertiseServices, FetchAssetResponse, MessageData, ParameterValues, RemoveStatus, ServerInfo,
    ServiceCallFailure, Unadvertise, UnadvertiseServices,
};
use ws_protocol::ParseError;

/// Identifies a client connection. Unique for the duration of the server's lifetime.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ClientId(u32);

impl From<ClientId> for u32 {
    fn from(client: ClientId) -> Self {
        client.0
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A connected client session with the websocket server.
#[derive(Debug, Clone)]
pub struct Client {
    id: ClientId,
    client: Weak<ConnectedClient>,
}

impl Client {
    pub(crate) fn new(client: &ConnectedClient) -> Self {
        Self {
            id: client.id,
            client: client.weak_self.clone(),
        }
    }

    /// Returns the client ID.
    pub fn id(&self) -> ClientId {
        self.id
    }

    /// Send a status message to this client. Does nothing if client is disconnected.
    pub fn send_status(&self, status: Status) {
        if let Some(client) = self.client.upgrade() {
            client.send_status(status);
        }
    }

    /// Send a fetch asset response to the client. Does nothing if client is disconnected.
    pub(crate) fn send_asset_response(&self, result: Result<Bytes, String>, request_id: u32) {
        if let Some(client) = self.client.upgrade() {
            match result {
                Ok(asset) => client.send_asset_response(&asset, request_id),
                Err(err) => client.send_asset_error(&err.to_string(), request_id),
            }
        }
    }
}

/// Information about a channel.
#[derive(Debug)]
pub struct ChannelView<'a> {
    id: ChannelId,
    topic: &'a str,
}

impl ChannelView<'_> {
    /// Returns the channel ID.
    pub fn id(&self) -> ChannelId {
        self.id
    }

    /// Returns the topic of the channel.
    pub fn topic(&self) -> &str {
        self.topic
    }
}

pub(crate) const SUBPROTOCOL: &str = "foxglove.sdk.v1";
const MAX_SEND_RETRIES: usize = 10;

type WebsocketSender = SplitSink<WebSocketStream<TcpStream>, Message>;

// Queue up to 1024 messages per connected client before dropping messages
// Can be overridden by ServerOptions::message_backlog_size.
const DEFAULT_MESSAGE_BACKLOG_SIZE: usize = 1024;
const DEFAULT_SERVICE_CALLS_PER_CLIENT: usize = 32;
const DEFAULT_FETCH_ASSET_CALLS_PER_CLIENT: usize = 32;

#[derive(Error, Debug)]
enum WSError {
    #[error("client handshake failed")]
    HandshakeError,
}

#[derive(Default)]
pub(crate) struct ServerOptions {
    pub session_id: Option<String>,
    pub name: Option<String>,
    pub message_backlog_size: Option<usize>,
    pub listener: Option<Arc<dyn ServerListener>>,
    pub capabilities: Option<HashSet<Capability>>,
    pub services: HashMap<String, Service>,
    pub supported_encodings: Option<HashSet<String>>,
    pub runtime: Option<Handle>,
    pub fetch_asset_handler: Option<Box<dyn AssetHandler>>,
}

impl std::fmt::Debug for ServerOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerOptions")
            .field("session_id", &self.session_id)
            .field("name", &self.name)
            .field("message_backlog_size", &self.message_backlog_size)
            .field("services", &self.services)
            .field("capabilities", &self.capabilities)
            .field("supported_encodings", &self.supported_encodings)
            .finish()
    }
}

/// A websocket server that implements the Foxglove WebSocket Protocol
pub(crate) struct Server {
    /// A weak reference to the Arc holding the server.
    /// This is used to get a reference to the outer `Arc<Server>` from Server methods.
    /// See the arc() method and its callers. We need the Arc so we can use it in async futures
    /// which need to prove to the compiler that the server will outlive the future.
    /// It's analogous to the mixin shared_from_this in C++.
    weak_self: Weak<Self>,
    started: AtomicBool,
    context: Weak<Context>,
    /// Local port the server is listening on, once it has been started
    port: AtomicU16,
    message_backlog_size: u32,
    runtime: Handle,
    /// May be provided by the caller
    session_id: parking_lot::RwLock<String>,
    name: String,
    clients: CowVec<Arc<ConnectedClient>>,
    /// Callbacks for handling client messages, etc.
    listener: Option<Arc<dyn ServerListener>>,
    /// Capabilities advertised to clients
    capabilities: HashSet<Capability>,
    /// Parameters subscribed to by clients
    subscribed_parameters: parking_lot::RwLock<HashMap<String, HashSet<ClientId>>>,
    /// Encodings server can accept from clients. Ignored unless the "clientPublish" capability is set.
    supported_encodings: HashSet<String>,
    /// The current connection graph, unused unless the "connectionGraph" capability is set.
    /// see https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#connection-graph-update
    connection_graph: parking_lot::Mutex<ConnectionGraph>,
    /// Token for cancelling all tasks
    cancellation_token: CancellationToken,
    /// Registered services.
    services: parking_lot::RwLock<ServiceMap>,
    /// Handler for fetch asset requests
    fetch_asset_handler: Option<Box<dyn AssetHandler>>,
}

/// Provides a mechanism for registering callbacks for handling client message events.
///
/// These methods are invoked from the client's main poll loop and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
pub trait ServerListener: Send + Sync {
    /// Callback invoked when a client message is received.
    fn on_message_data(&self, _client: Client, _client_channel: &ClientChannel, _payload: &[u8]) {}
    /// Callback invoked when a client subscribes to a channel.
    /// Only invoked if the channel is associated with the server and isn't already subscribed to by the client.
    fn on_subscribe(&self, _client: Client, _channel: ChannelView) {}
    /// Callback invoked when a client unsubscribes from a channel or disconnects.
    /// Only invoked for channels that had an active subscription from the client.
    fn on_unsubscribe(&self, _client: Client, _channel: ChannelView) {}
    /// Callback invoked when a client advertises a client channel. Requires [`Capability::ClientPublish`].
    fn on_client_advertise(&self, _client: Client, _channel: &ClientChannel) {}
    /// Callback invoked when a client unadvertises a client channel. Requires [`Capability::ClientPublish`].
    fn on_client_unadvertise(&self, _client: Client, _channel: &ClientChannel) {}
    /// Callback invoked when a client requests parameters. Requires [`Capability::Parameters`].
    /// Should return the named paramters, or all paramters if param_names is empty.
    fn on_get_parameters(
        &self,
        _client: Client,
        _param_names: Vec<String>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        Vec::new()
    }
    /// Callback invoked when a client sets parameters. Requires [`Capability::Parameters`].
    /// Should return the updated parameters for the passed parameters.
    /// The implementation could return the modified parameters.
    /// All clients subscribed to updates for the _returned_ parameters will be notified.
    ///
    /// Note that only `parameters` which have changed are included in the callback, but the return
    /// value must include all parameters.
    fn on_set_parameters(
        &self,
        _client: Client,
        parameters: Vec<Parameter>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        parameters
    }
    /// Callback invoked when a client subscribes to the named parameters for the first time.
    /// Requires [`Capability::Parameters`].
    fn on_parameters_subscribe(&self, _param_names: Vec<String>) {}
    /// Callback invoked when the last client unsubscribes from the named parameters.
    /// Requires [`Capability::Parameters`].
    fn on_parameters_unsubscribe(&self, _param_names: Vec<String>) {}
    /// Callback invoked when the first client subscribes to the connection graph. Requires [`Capability::ConnectionGraph`].
    fn on_connection_graph_subscribe(&self) {}
    /// Callback invoked when the last client unsubscribes from the connection graph. Requires [`Capability::ConnectionGraph`].
    fn on_connection_graph_unsubscribe(&self) {}
}

/// A connected client session with the websocket server.
pub(crate) struct ConnectedClient {
    id: ClientId,
    addr: SocketAddr,
    weak_self: Weak<Self>,
    sink_id: SinkId,
    context: Weak<Context>,
    /// A cache of channels for `on_subscribe` and `on_unsubscribe` callbacks.
    channels: parking_lot::RwLock<HashMap<ChannelId, Arc<RawChannel>>>,
    /// Write side of a WS stream
    sender: Mutex<WebsocketSender>,
    data_plane_tx: flume::Sender<Message>,
    data_plane_rx: flume::Receiver<Message>,
    control_plane_tx: flume::Sender<Message>,
    control_plane_rx: flume::Receiver<Message>,
    service_call_sem: Semaphore,
    fetch_asset_sem: Semaphore,
    /// Subscriptions from this client
    subscriptions: parking_lot::Mutex<BiHashMap<ChannelId, SubscriptionId>>,
    /// Channels advertised by this client
    advertised_channels: parking_lot::Mutex<HashMap<ClientChannelId, Arc<ClientChannel>>>,
    /// Optional callback handler for a server implementation
    server_listener: Option<Arc<dyn ServerListener>>,
    server: Weak<Server>,
    /// The cancellation_token is used by the server to disconnect the client.
    /// It's cancelled when the client's control plane queue fills up (slow client).
    cancellation_token: CancellationToken,
}

impl ConnectedClient {
    fn id(&self) -> ClientId {
        self.id
    }

    fn arc(&self) -> Arc<Self> {
        self.weak_self
            .upgrade()
            .expect("client cannot be dropped while in use")
    }

    /// Handle a text or binary message sent from the client.
    ///
    /// Standard protocol messages (such as Close) should be handled upstream.
    fn handle_message(&self, message: Message) {
        let msg = match ClientMessage::try_from(&message) {
            Ok(m) => m,
            Err(ParseError::EmptyBinaryMessage) => {
                tracing::debug!("Received empty binary message from {}", self.addr);
                return;
            }
            Err(ParseError::UnhandledMessageType) => {
                tracing::debug!("Unhandled websocket message: {message:?}");
                return;
            }
            Err(err) => {
                tracing::error!("Invalid message from {}: {err}", self.addr);
                tracing::debug!("Invalid message: {message:?}");
                self.send_error(format!("Invalid message: {err}"));
                return;
            }
        };

        let Some(server) = self.server.upgrade() else {
            return;
        };

        match msg {
            ClientMessage::Subscribe(msg) => self.on_subscribe(msg),
            ClientMessage::Unsubscribe(msg) => self.on_unsubscribe(msg),
            ClientMessage::Advertise(msg) => self.on_advertise(server, msg),
            ClientMessage::Unadvertise(msg) => self.on_unadvertise(msg),
            ClientMessage::MessageData(msg) => self.on_message_data(msg),
            ClientMessage::GetParameters(msg) => {
                self.on_get_parameters(server, msg.parameter_names, msg.id)
            }
            ClientMessage::SetParameters(msg) => {
                self.on_set_parameters(server, msg.parameters, msg.id)
            }
            ClientMessage::SubscribeParameterUpdates(msg) => {
                self.on_parameters_subscribe(server, msg.parameter_names)
            }
            ClientMessage::UnsubscribeParameterUpdates(msg) => {
                self.on_parameters_unsubscribe(server, msg.parameter_names)
            }
            ClientMessage::ServiceCallRequest(msg) => self.on_service_call(msg),
            ClientMessage::FetchAsset(msg) => self.on_fetch_asset(server, msg.uri, msg.request_id),
            ClientMessage::SubscribeConnectionGraph => self.on_connection_graph_subscribe(server),
            ClientMessage::UnsubscribeConnectionGraph => {
                self.on_connection_graph_unsubscribe(server)
            }
        }
    }

    /// Send the message on the data plane, dropping up to retries older messages to make room, if necessary.
    fn send_data_lossy(&self, message: impl Into<Message>, retries: usize) -> SendLossyResult {
        send_lossy(
            &self.addr,
            &self.data_plane_tx,
            &self.data_plane_rx,
            message.into(),
            retries,
        )
    }

    /// Send the message on the control plane, disconnecting the client if the channel is full.
    fn send_control_msg(&self, message: impl Into<Message>) -> bool {
        if let Err(TrySendError::Full(_)) = self.control_plane_tx.try_send(message.into()) {
            self.cancellation_token.cancel();
            return false;
        }
        true
    }

    async fn on_disconnect(&self) {
        if self.cancellation_token.is_cancelled() {
            let mut sender = self.sender.lock().await;
            let status = Status::new(
                StatusLevel::Error,
                "Disconnected because message backlog on the server is full. The backlog size is configurable in the server setup."
                    .to_string(),
            );
            let message = Message::text(serde_json::to_string(&status).unwrap());
            _ = sender.send(message).await;
            _ = sender.send(Message::Close(None)).await;
        }

        let channel_ids = {
            let subscriptions = self.subscriptions.lock();
            subscriptions.left_values().copied().collect()
        };
        self.unsubscribe_channel_ids(channel_ids);
    }

    fn on_message_data(&self, message: ws_protocol::client::MessageData) {
        let channel_id = ClientChannelId::new(message.channel_id);
        let payload = message.data;
        let client_channel = {
            let advertised_channels = self.advertised_channels.lock();
            let Some(channel) = advertised_channels.get(&channel_id) else {
                tracing::error!("Received message for unknown channel: {}", channel_id);
                self.send_error(format!("Unknown channel ID: {}", channel_id));
                // Do not forward to server listener
                return;
            };
            channel.clone()
        };
        // Call the handler after releasing the advertised_channels lock
        if let Some(handler) = self.server_listener.as_ref() {
            handler.on_message_data(Client::new(self), &client_channel, &payload);
        }
    }

    fn on_unadvertise(&self, message: ws_protocol::client::Unadvertise) {
        let mut channel_ids: Vec<_> = message
            .channel_ids
            .into_iter()
            .map(ClientChannelId::new)
            .collect();
        let mut client_channels = Vec::with_capacity(channel_ids.len());
        // Using a limited scope and iterating twice to avoid holding the lock on advertised_channels while calling on_client_unadvertise
        {
            let mut advertised_channels = self.advertised_channels.lock();
            let mut i = 0;
            while i < channel_ids.len() {
                let id = channel_ids[i];
                let Some(channel) = advertised_channels.remove(&id) else {
                    // Remove the channel ID from the list so we don't invoke the on_client_unadvertise callback
                    channel_ids.swap_remove(i);
                    self.send_warning(format!(
                        "Client is not advertising channel: {}; ignoring unadvertisement",
                        id
                    ));
                    continue;
                };
                client_channels.push(channel.clone());
                i += 1;
            }
        }
        // Call the handler after releasing the advertised_channels lock
        if let Some(handler) = self.server_listener.as_ref() {
            for client_channel in client_channels {
                handler.on_client_unadvertise(Client::new(self), &client_channel);
            }
        }
    }

    fn on_advertise(&self, server: Arc<Server>, message: ws_protocol::client::Advertise) {
        if !server.capabilities.contains(&Capability::ClientPublish) {
            self.send_error("Server does not support clientPublish capability".to_string());
            return;
        }

        let channels: Vec<_> = message
            .channels
            .into_iter()
            .filter_map(|c| {
                ClientChannel::try_from(c)
                    .inspect_err(|e| tracing::warn!("Failed to parse advertised channel: {e:?}"))
                    .ok()
            })
            .collect();

        for channel in channels {
            // Using a limited scope here to avoid holding the lock on advertised_channels while calling on_client_advertise
            let client_channel = {
                match self.advertised_channels.lock().entry(channel.id) {
                    Entry::Occupied(_) => {
                        self.send_warning(format!(
                            "Client is already advertising channel: {}; ignoring advertisement",
                            channel.id
                        ));
                        continue;
                    }
                    Entry::Vacant(entry) => {
                        let client_channel = Arc::new(channel);
                        entry.insert(client_channel.clone());
                        client_channel
                    }
                }
            };

            // Call the handler after releasing the advertised_channels lock
            if let Some(handler) = self.server_listener.as_ref() {
                handler.on_client_advertise(Client::new(self), &client_channel);
            }
        }
    }

    fn on_unsubscribe(&self, message: ws_protocol::client::Unsubscribe) {
        let subscription_ids: Vec<_> = message
            .subscription_ids
            .into_iter()
            .map(SubscriptionId::new)
            .collect();

        let mut unsubscribed_channel_ids = Vec::with_capacity(subscription_ids.len());
        // First gather the unsubscribed channel ids while holding the subscriptions lock
        {
            let mut subscriptions = self.subscriptions.lock();
            for subscription_id in subscription_ids {
                if let Some((channel_id, _)) = subscriptions.remove_by_right(&subscription_id) {
                    unsubscribed_channel_ids.push(channel_id);
                }
            }
        }

        self.unsubscribe_channel_ids(unsubscribed_channel_ids);
    }

    fn on_subscribe(&self, message: ws_protocol::client::Subscribe) {
        let mut subscriptions: Vec<_> = message
            .subscriptions
            .into_iter()
            .map(Subscription::from)
            .collect();

        // First prune out any subscriptions for channels not in the channel map,
        // limiting how long we need to hold the lock.
        let mut subscribed_channels = Vec::with_capacity(subscriptions.len());
        {
            let channels = self.channels.read();
            let mut i = 0;
            while i < subscriptions.len() {
                let subscription = &subscriptions[i];
                let Some(channel) = channels.get(&subscription.channel_id) else {
                    tracing::error!(
                        "Client {} attempted to subscribe to unknown channel: {}",
                        self.addr,
                        subscription.channel_id
                    );
                    self.send_error(format!("Unknown channel ID: {}", subscription.channel_id));
                    // Remove the subscription from the list so we don't invoke the on_subscribe callback for it
                    subscriptions.swap_remove(i);
                    continue;
                };
                subscribed_channels.push(channel.clone());
                i += 1
            }
        }

        let mut channel_ids = Vec::with_capacity(subscribed_channels.len());
        for (subscription, channel) in subscriptions.into_iter().zip(subscribed_channels) {
            // Using a limited scope here to avoid holding the lock on subscriptions while calling on_subscribe
            {
                let mut subscriptions = self.subscriptions.lock();
                if subscriptions
                    .insert_no_overwrite(subscription.channel_id, subscription.id)
                    .is_err()
                {
                    if subscriptions.contains_left(&subscription.channel_id) {
                        self.send_warning(format!(
                            "Client is already subscribed to channel: {}; ignoring subscription",
                            subscription.channel_id
                        ));
                    } else {
                        assert!(subscriptions.contains_right(&subscription.id));
                        self.send_error(format!(
                            "Subscription ID was already used: {}; ignoring subscription",
                            subscription.id
                        ));
                    }
                    continue;
                }
            }

            tracing::debug!(
                "Client {} subscribed to channel {} with subscription id {}",
                self.addr,
                subscription.channel_id,
                subscription.id
            );
            channel_ids.push(channel.id());

            if let Some(handler) = self.server_listener.as_ref() {
                handler.on_subscribe(
                    Client::new(self),
                    ChannelView {
                        id: channel.id(),
                        topic: channel.topic(),
                    },
                );
            }
        }

        // Propagate client subscription requests to the context.
        if let Some(context) = self.context.upgrade() {
            context.subscribe_channels(self.sink_id, &channel_ids);
        }
    }

    fn on_get_parameters(
        &self,
        server: Arc<Server>,
        param_names: Vec<String>,
        request_id: Option<String>,
    ) {
        if !server.capabilities.contains(&Capability::Parameters) {
            self.send_error("Server does not support parameters capability".to_string());
            return;
        }

        if let Some(handler) = self.server_listener.as_ref() {
            let parameters =
                handler.on_get_parameters(Client::new(self), param_names, request_id.as_deref());
            let mut msg = ParameterValues::new(parameters);
            if let Some(id) = request_id {
                msg = msg.with_id(id);
            }
            self.send_control_msg(&msg);
        }
    }

    fn on_set_parameters(
        &self,
        server: Arc<Server>,
        parameters: Vec<Parameter>,
        request_id: Option<String>,
    ) {
        if !server.capabilities.contains(&Capability::Parameters) {
            self.send_error("Server does not support parameters capability".to_string());
            return;
        }

        let updated_parameters = if let Some(handler) = self.server_listener.as_ref() {
            let updated =
                handler.on_set_parameters(Client::new(self), parameters, request_id.as_deref());
            // Send all the updated_parameters back to the client if request_id is provided.
            // This is the behavior of the reference Python server implementation.
            if let Some(id) = request_id {
                self.send_control_msg(&ParameterValues::new(updated.clone()).with_id(id));
            }
            updated
        } else {
            // This differs from the Python legacy ws-protocol implementation in that here we notify
            // subscribers about the parameters even if there's no ServerListener configured.
            // This seems to be a more sensible default.
            parameters
        };
        server.publish_parameter_values(updated_parameters);
    }

    fn update_parameters(&self, parameters: Vec<Parameter>) {
        self.send_control_msg(&ParameterValues::new(parameters));
    }

    fn on_parameters_subscribe(&self, server: Arc<Server>, names: Vec<String>) {
        if server.has_capability(Capability::Parameters) {
            server.subscribe_parameters(self.id, names);
        } else {
            self.send_error("Server does not support parametersSubscribe capability".to_string());
        }
    }

    fn on_parameters_unsubscribe(&self, server: Arc<Server>, names: Vec<String>) {
        if server.has_capability(Capability::Parameters) {
            server.unsubscribe_parameters(self.id, names);
        } else {
            self.send_error("Server does not support parametersSubscribe capability".to_string());
        }
    }

    fn on_service_call(&self, req: ws_protocol::client::ServiceCallRequest) {
        let Some(server) = self.server.upgrade() else {
            return;
        };

        // We have a response channel if and only if the server supports services.
        let service_id = ServiceId::new(req.service_id);
        let call_id = CallId::new(req.call_id);
        if !server.capabilities.contains(&Capability::Services) {
            self.send_service_call_failure(service_id, call_id, "Server does not support services");
            return;
        };

        // Lookup the requested service handler.
        let Some(service) = server.get_service(service_id) else {
            self.send_service_call_failure(service_id, call_id, "Unknown service");
            return;
        };

        // If this service declared a request encoding, ensure that it matches. Otherwise, ensure
        // that the request encoding is in the server's global list of supported encodings.
        if !service
            .request_encoding()
            .map(|e| e == req.encoding)
            .unwrap_or_else(|| server.supported_encodings.contains(req.encoding.as_ref()))
        {
            self.send_service_call_failure(service_id, call_id, "Unsupported encoding");
            return;
        }

        // Acquire the semaphore, or reject if there are too many concurrenct requests.
        let Some(guard) = self.service_call_sem.try_acquire() else {
            self.send_service_call_failure(service_id, call_id, "Too many requests");
            return;
        };

        // Prepare the responder and the request. No failures past this point. If the responder is
        // dropped without sending a response, it will send a generic "internal server error" back
        // to the client.
        let responder = service::Responder::new(
            self.arc(),
            service.id(),
            call_id,
            service.response_encoding().unwrap_or(&req.encoding),
            guard,
        );
        let request = service::Request::new(
            service.clone(),
            self.id,
            call_id,
            req.encoding.into_owned(),
            req.payload.into_owned().into(),
        );

        // Invoke the handler.
        service.call(request, responder);
    }

    /// Sends a service call failure message to the client with the provided message.
    fn send_service_call_failure(&self, service_id: ServiceId, call_id: CallId, message: &str) {
        self.send_control_msg(&ServiceCallFailure::new(
            service_id.into(),
            call_id.into(),
            message,
        ));
    }

    fn on_fetch_asset(&self, server: Arc<Server>, uri: String, request_id: u32) {
        if !server.capabilities.contains(&Capability::Assets) {
            self.send_error("Server does not support assets capability".to_string());
            return;
        }

        let Some(guard) = self.fetch_asset_sem.try_acquire() else {
            self.send_asset_error("Too many concurrent fetch asset requests", request_id);
            return;
        };

        if let Some(handler) = server.fetch_asset_handler.as_ref() {
            let asset_responder = AssetResponder::new(Client::new(self), request_id, guard);
            handler.fetch(uri, asset_responder);
        } else {
            tracing::error!("Server advertised the Assets capability without providing a handler");
            self.send_asset_error("Server does not have a fetch asset handler", request_id);
        }
    }

    fn on_connection_graph_subscribe(&self, server: Arc<Server>) {
        if !server.has_capability(Capability::ConnectionGraph) {
            self.send_error("Server does not support connection graph capability".to_string());
            return;
        }

        if let Some(initial_update) = server.subscribe_connection_graph(self.id) {
            self.send_control_msg(initial_update);
        } else {
            tracing::debug!(
                "Client {} is already subscribed to connection graph updates",
                self.addr
            );
        }
    }

    fn on_connection_graph_unsubscribe(&self, server: Arc<Server>) {
        if !server.capabilities.contains(&Capability::ConnectionGraph) {
            self.send_error("Server does not support connection graph capability".to_string());
            return;
        }

        if !server.unsubscribe_connection_graph(self.id) {
            tracing::debug!(
                "Client {} is already unsubscribed from connection graph updates",
                self.addr
            );
        }
    }

    /// Send an ad hoc error status message to the client, with the given message.
    fn send_error(&self, message: String) {
        tracing::debug!("Sending error to client {}: {}", self.addr, message);
        self.send_status(Status::error(message));
    }

    /// Send an ad hoc warning status message to the client, with the given message.
    fn send_warning(&self, message: String) {
        tracing::debug!("Sending warning to client {}: {}", self.addr, message);
        self.send_status(Status::warning(message));
    }

    /// Send a status message to the client.
    fn send_status(&self, status: Status) {
        match status.level {
            StatusLevel::Info => {
                self.send_data_lossy(&status, MAX_SEND_RETRIES);
            }
            _ => {
                self.send_control_msg(&status);
            }
        }
    }

    /// Send a fetch asset error to the client.
    fn send_asset_error(&self, error: &str, request_id: u32) {
        self.send_control_msg(&FetchAssetResponse::error_message(request_id, error));
    }

    /// Send a fetch asset response to the client.
    fn send_asset_response(&self, response: &[u8], request_id: u32) {
        self.send_control_msg(&FetchAssetResponse::asset_data(request_id, response));
    }

    /// Advertises a channel to the client.
    fn advertise_channel(&self, channel: &Arc<RawChannel>) {
        let message = advertise::advertise_channels([channel]);
        if message.channels.is_empty() {
            return;
        }

        self.channels.write().insert(channel.id(), channel.clone());

        if self.send_control_msg(&message) {
            tracing::debug!(
                "Advertised channel {} with id {} to client {}",
                channel.topic(),
                channel.id(),
                self.addr
            );
        }
    }

    /// Unadvertises a channel to the client.
    fn unadvertise_channel(&self, channel_id: ChannelId) {
        self.channels.write().remove(&channel_id);

        let message = Unadvertise::new([channel_id.into()]);

        if self.send_control_msg(&message) {
            tracing::debug!(
                "Unadvertised channel with id {} to client {}",
                channel_id,
                self.addr
            );
        }
    }

    /// Unsubscribes from a list of channel IDs.
    /// Takes a read lock on the channels map.
    fn unsubscribe_channel_ids(&self, unsubscribed_channel_ids: Vec<ChannelId>) {
        // Propagate client unsubscriptions to the context.
        if let Some(context) = self.context.upgrade() {
            context.unsubscribe_channels(self.sink_id, &unsubscribed_channel_ids);
        }

        // If we don't have a ServerListener, we're done.
        let Some(handler) = self.server_listener.as_ref() else {
            return;
        };

        // Then gather the actual channel references while holding the channels lock
        let mut unsubscribed_channels = Vec::with_capacity(unsubscribed_channel_ids.len());
        {
            let channels = self.channels.read();
            for channel_id in unsubscribed_channel_ids {
                if let Some(channel) = channels.get(&channel_id) {
                    unsubscribed_channels.push(channel.clone());
                }
            }
        }

        // Finally call the handler for each channel
        for channel in unsubscribed_channels {
            handler.on_unsubscribe(
                Client::new(self),
                ChannelView {
                    id: channel.id(),
                    topic: channel.topic(),
                },
            );
        }
    }
}

impl std::fmt::Debug for ConnectedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("id", &self.id)
            .field("address", &self.addr)
            .finish()
    }
}

// A websocket server that implements the Foxglove WebSocket Protocol
impl Server {
    /// Generate a random session ID
    pub(crate) fn generate_session_id() -> String {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default()
    }

    pub fn new(weak_self: Weak<Self>, ctx: &Arc<Context>, opts: ServerOptions) -> Self {
        let mut capabilities = opts.capabilities.unwrap_or_default();
        let mut supported_encodings = opts.supported_encodings.unwrap_or_default();

        // If the server was declared with services, automatically add the "services" capability
        // and the set of supported request encodings.
        if !opts.services.is_empty() {
            capabilities.insert(Capability::Services);
            supported_encodings.extend(
                opts.services
                    .values()
                    .filter_map(|svc| svc.schema().request().map(|s| s.encoding.clone())),
            );
        }

        // If the server was declared with fetch asset handler, automatically add the "assets" capability
        if opts.fetch_asset_handler.is_some() {
            capabilities.insert(Capability::Assets);
        }

        Server {
            weak_self,
            port: AtomicU16::new(0),
            started: AtomicBool::new(false),
            context: Arc::downgrade(ctx),
            message_backlog_size: opts
                .message_backlog_size
                .unwrap_or(DEFAULT_MESSAGE_BACKLOG_SIZE) as u32,
            runtime: opts.runtime.unwrap_or_else(get_runtime_handle),
            listener: opts.listener,
            session_id: parking_lot::RwLock::new(
                opts.session_id.unwrap_or_else(Self::generate_session_id),
            ),
            name: opts.name.unwrap_or_default(),
            clients: CowVec::new(),
            subscribed_parameters: parking_lot::RwLock::default(),
            capabilities,
            supported_encodings,
            connection_graph: parking_lot::Mutex::default(),
            cancellation_token: CancellationToken::new(),
            services: parking_lot::RwLock::new(ServiceMap::from_iter(opts.services.into_values())),
            fetch_asset_handler: opts.fetch_asset_handler,
        }
    }

    pub fn arc(&self) -> Arc<Self> {
        self.weak_self
            .upgrade()
            .expect("server cannot be dropped while in use")
    }

    pub(crate) fn port(&self) -> u16 {
        self.port.load(Acquire)
    }

    // Returns a handle to the async runtime that this server is using.
    pub fn runtime(&self) -> &Handle {
        &self.runtime
    }

    /// Returns true if the server supports the capability.
    fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    // Spawn a task to accept all incoming connections and return the server's local address
    pub async fn start(&self, host: &str, port: u16) -> Result<SocketAddr, FoxgloveError> {
        if self.started.load(Acquire) {
            return Err(FoxgloveError::ServerAlreadyStarted);
        }
        let already_started = self.started.swap(true, AcqRel);
        assert!(!already_started);

        let addr = format!("{}:{}", host, port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(FoxgloveError::Bind)?;
        let local_addr = listener.local_addr().map_err(FoxgloveError::Bind)?;
        self.port.store(local_addr.port(), Release);

        let cancellation_token = self.cancellation_token.clone();
        let server = self.arc().clone();
        self.runtime.spawn(async move {
            tokio::select! {
                () = handle_connections(server, listener) => (),
                () = cancellation_token.cancelled() => {
                    tracing::debug!("Closed connection handler");
                }
            }
        });

        tracing::info!("Started server on {}", local_addr);

        Ok(local_addr)
    }

    pub async fn stop(&self) {
        if self
            .started
            .compare_exchange(true, false, AcqRel, Acquire)
            .is_err()
        {
            return;
        }
        tracing::info!("Shutting down");
        self.port.store(0, Release);
        let clients = self.clients.get();
        for client in clients.iter() {
            let mut sender = client.sender.lock().await;
            sender.send(Message::Close(None)).await.ok();
        }
        self.clients.clear();
        self.cancellation_token.cancel();
    }

    /// Publish the current timestamp to all clients.
    #[cfg(feature = "unstable")]
    pub fn broadcast_time(&self, timestamp: u64) {
        use ws_protocol::server::Time;

        if !self.capabilities.contains(&Capability::Time) {
            tracing::error!("Server does not support time capability");
            return;
        }

        let message = Time::new(timestamp);
        let clients = self.clients.get();
        for client in clients.iter() {
            client.send_control_msg(&message);
        }
    }

    /// Adds client parameter subscriptions by parameter name.
    fn subscribe_parameters(&self, client_id: ClientId, names: Vec<String>) {
        let mut subs = self.subscribed_parameters.write();

        // Update subscriptions, keeping track of params that are newly-subscribed.
        let mut new_names = vec![];
        for name in names {
            match subs.entry(name.clone()) {
                Entry::Occupied(mut entry) => {
                    entry.get_mut().insert(client_id);
                }
                Entry::Vacant(entry) => {
                    entry.insert(HashSet::from_iter([client_id]));
                    new_names.push(name);
                }
            }
        }

        // Notify listener.
        if !new_names.is_empty() {
            if let Some(listener) = self.listener.as_ref() {
                listener.on_parameters_subscribe(new_names);
            }
        }
    }

    /// Removes client parameter subscriptions by parameter name.
    fn unsubscribe_parameters(&self, client_id: ClientId, names: Vec<String>) {
        let mut subs = self.subscribed_parameters.write();

        // Update subscriptions, keeping track of params that now have no subscribers.
        let mut old_names = vec![];
        for name in names {
            if let Some(entry) = subs.get_mut(&name) {
                if entry.remove(&client_id) && entry.is_empty() {
                    subs.remove(&name);
                    old_names.push(name);
                }
            }
        }

        // Notify listener.
        if !old_names.is_empty() {
            if let Some(listener) = self.listener.as_ref() {
                listener.on_parameters_unsubscribe(old_names);
            }
        }
    }

    /// Removes all client parameter subscriptions.
    fn unsubscribe_all_parameters(&self, client_id: ClientId) {
        let mut subs = self.subscribed_parameters.write();

        // Update subscriptions, keeping track of params that now have no subscribers.
        let mut old_names = vec![];
        for (name, entry) in subs.iter_mut() {
            if entry.remove(&client_id) && entry.is_empty() {
                old_names.push(name.to_string());
            }
        }
        for name in &old_names {
            subs.remove(name);
        }

        // Notify listener.
        if !old_names.is_empty() {
            if let Some(listener) = self.listener.as_ref() {
                listener.on_parameters_unsubscribe(old_names);
            }
        }
    }

    /// Adds a connection graph subscription for the client.
    ///
    /// Returns `None` if this client is already subscribed. Otherwise, returns an initial
    /// `ConnectionGraphUpdate` message with the complete graph state.
    fn subscribe_connection_graph(&self, client_id: ClientId) -> Option<Message> {
        let mut graph = self.connection_graph.lock();
        let first = !graph.has_subscribers();
        if !graph.add_subscriber(client_id) {
            return None;
        }

        // Notify listener, if this is the first subscriber.
        if first {
            if let Some(listener) = self.listener.as_ref() {
                listener.on_connection_graph_subscribe();
            }
        }

        let initial_update = Message::from(&graph.as_initial_update());
        Some(initial_update)
    }

    /// Removes a connection graph subscription for the client.
    ///
    /// Returns false if this client is already unsubscribed.
    fn unsubscribe_connection_graph(&self, client_id: ClientId) -> bool {
        let mut graph = self.connection_graph.lock();
        if !graph.remove_subscriber(client_id) {
            return false;
        }

        // Notify listener, if this was the last subscriber.
        if !graph.has_subscribers() {
            if let Some(listener) = self.listener.as_ref() {
                listener.on_connection_graph_unsubscribe();
            }
        }

        true
    }

    /// Publish parameter values to all subscribed clients.
    pub fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        if !self.has_capability(Capability::Parameters) {
            tracing::error!("Server does not support parameters capability");
            return;
        }

        let clients = self.clients.get();
        for client in clients.iter() {
            // Filter parameters by subscriptions.
            let filtered: Vec<_> = {
                let subs = self.subscribed_parameters.read();
                parameters
                    .iter()
                    .filter(|p| {
                        subs.get(&p.name)
                            .is_some_and(|ids| ids.contains(&client.id()))
                    })
                    .cloned()
                    .collect()
            };

            // Notify client.
            if !filtered.is_empty() {
                client.update_parameters(filtered);
            }
        }
    }

    /// Send a status message to all clients.
    pub fn publish_status(&self, status: Status) {
        let clients = self.clients.get();
        for client in clients.iter() {
            client.send_status(status.clone());
        }
    }

    /// Remove status messages by id from all clients.
    pub fn remove_status(&self, status_ids: Vec<String>) {
        let message = RemoveStatus { status_ids };
        let clients = self.clients.get();
        for client in clients.iter() {
            client.send_control_msg(&message);
        }
    }

    /// Builds a server info message.
    fn server_info(&self) -> ServerInfo {
        ServerInfo::new(&self.name)
            .with_capabilities(
                self.capabilities
                    .iter()
                    .flat_map(Capability::as_protocol_capabilities)
                    .copied(),
            )
            .with_supported_encodings(&self.supported_encodings)
            .with_session_id(self.session_id.read().clone())
    }

    /// Sets a new session ID and notifies all clients, causing them to reset their state.
    /// If no session ID is provided, generates a new one based on the current timestamp.
    pub fn clear_session(&self, new_session_id: Option<String>) {
        *self.session_id.write() = new_session_id.unwrap_or_else(Self::generate_session_id);

        let message = self.server_info();
        let clients = self.clients.get();
        for client in clients.iter() {
            client.send_control_msg(&message);
        }
    }

    /// When a new client connects:
    /// - Handshake
    /// - Send ServerInfo
    /// - Advertise existing channels
    /// - Advertise existing services
    /// - Listen for client meesages
    async fn handle_connection(self: Arc<Self>, stream: TcpStream, addr: SocketAddr) {
        let Ok(ws_stream) = do_handshake(stream).await else {
            tracing::error!("Dropping client {addr}: {}", WSError::HandshakeError);
            return;
        };

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        let message = Message::from(&self.server_info());
        if let Err(err) = ws_sender.send(message).await {
            // ServerInfo is required; do not store this client.
            tracing::error!("Failed to send required server info: {err}");
            return;
        }

        static CLIENT_ID: AtomicU32 = AtomicU32::new(1);
        let id = ClientId(CLIENT_ID.fetch_add(1, AcqRel));

        let (data_tx, data_rx) = flume::bounded(self.message_backlog_size as usize);
        let (ctrl_tx, ctrl_rx) = flume::bounded(self.message_backlog_size as usize);
        let cancellation_token = CancellationToken::new();

        let sink_id = SinkId::next();
        let new_client = Arc::new_cyclic(|weak_self| ConnectedClient {
            id,
            addr,
            weak_self: weak_self.clone(),
            sink_id,
            context: self.context.clone(),
            channels: parking_lot::RwLock::default(),
            sender: Mutex::new(ws_sender),
            data_plane_tx: data_tx,
            data_plane_rx: data_rx,
            control_plane_tx: ctrl_tx,
            control_plane_rx: ctrl_rx,
            service_call_sem: Semaphore::new(DEFAULT_SERVICE_CALLS_PER_CLIENT),
            fetch_asset_sem: Semaphore::new(DEFAULT_FETCH_ASSET_CALLS_PER_CLIENT),
            subscriptions: parking_lot::Mutex::new(BiHashMap::new()),
            advertised_channels: parking_lot::Mutex::new(HashMap::new()),
            server_listener: self.listener.clone(),
            server: self.weak_self.clone(),
            cancellation_token: cancellation_token.clone(),
        });

        self.register_client_and_advertise(new_client.clone());

        let receive_messages = async {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(Message::Close(_)) => {
                        tracing::info!("Connection closed by client {addr}");
                        // Finish receive_messages
                        return;
                    }
                    Ok(msg) => {
                        new_client.handle_message(msg);
                    }
                    Err(err) => {
                        tracing::error!("Error receiving from client {addr}: {err}");
                    }
                }
            }
        };

        let send_control_messages = async {
            while let Ok(msg) = new_client.control_plane_rx.recv_async().await {
                let mut sender = new_client.sender.lock().await;
                if let Err(err) = sender.send(msg).await {
                    if self.started.load(Acquire) {
                        tracing::error!("Error sending control message to client {addr}: {err}");
                    } else {
                        new_client.control_plane_rx.drain();
                        new_client.data_plane_rx.drain();
                    }
                }
            }
        };

        // send_messages forwards messages from the rx size of the data plane to the sender
        let send_messages = async {
            while let Ok(msg) = new_client.data_plane_rx.recv_async().await {
                let mut sender = new_client.sender.lock().await;
                if let Err(err) = sender.send(msg).await {
                    if self.started.load(Acquire) {
                        tracing::error!("Error sending data message to client {addr}: {err}");
                    } else {
                        new_client.control_plane_rx.drain();
                        new_client.data_plane_rx.drain();
                    }
                }
            }
        };

        // Run send and receive loops concurrently, and wait for receive to complete
        tokio::select! {
            _ = send_control_messages => {
                tracing::error!("Send control messages task completed");
            }
            _ = send_messages => {
                tracing::error!("Send messages task completed");
            }
            _ = receive_messages => {
                tracing::debug!("Receive messages task completed");
            }
            _ = cancellation_token.cancelled() => {
                tracing::warn!("Server disconnecting slow client {}", new_client.addr);
            }
        }

        // Remove the client sink.
        if let Some(context) = self.context.upgrade() {
            context.remove_sink(sink_id);
        }

        self.clients.retain(|c| !Arc::ptr_eq(c, &new_client));
        if self.has_capability(Capability::Parameters) {
            self.unsubscribe_all_parameters(new_client.id());
        }
        if self.has_capability(Capability::ConnectionGraph) {
            self.unsubscribe_connection_graph(new_client.id());
        }
        new_client.on_disconnect().await;
    }

    fn register_client_and_advertise(&self, client: Arc<ConnectedClient>) {
        // Add the client to self.clients, and register it as a sink. This synchronously triggers
        // advertisements for all channels via the `Sink::add_channel` callback.
        tracing::info!("Registered client {}", client.addr);
        self.clients.push(client.clone());
        if let Some(context) = self.context.upgrade() {
            context.add_sink(client.clone());
        }

        // Advertise services.
        let services: Vec<_> = self.services.read().values().cloned().collect();
        let msg = advertise::advertise_services(services.iter().map(|s| s.as_ref()));
        if msg.services.is_empty() {
            return;
        }
        if client.send_control_msg(&msg) {
            for service in services {
                tracing::debug!(
                    "Advertised service {} with id {} to client {}",
                    service.name(),
                    service.id(),
                    client.addr
                );
            }
        }
    }

    /// Adds new services, and advertises them to all clients.
    ///
    /// This method will fail if the services capability was not declared, or if a service name is
    /// not unique.
    pub fn add_services(&self, new_services: Vec<Service>) -> Result<(), FoxgloveError> {
        // Make sure that the server supports services.
        if !self.capabilities.contains(&Capability::Services) {
            return Err(FoxgloveError::ServicesNotSupported);
        }
        if new_services.is_empty() {
            return Ok(());
        }

        let mut new_names = HashMap::with_capacity(new_services.len());
        let mut msg = AdvertiseServices { services: vec![] };
        for service in &new_services {
            // Ensure that the new service names are unique.
            if new_names
                .insert(service.name().to_string(), service.id())
                .is_some()
            {
                return Err(FoxgloveError::DuplicateService(service.name().to_string()));
            }

            // If the service doesn't declare a request encoding, there must be at least one
            // encoding declared in the global list.
            if service.request_encoding().is_none() && self.supported_encodings.is_empty() {
                return Err(FoxgloveError::MissingRequestEncoding(
                    service.name().to_string(),
                ));
            }

            // Prepare a service advertisement.
            if let Some(adv) = advertise::maybe_advertise_service(service) {
                msg.services.push(adv.into_owned());
            }
        }

        {
            // Ensure that the new services are not already registered.
            let mut services = self.services.write();
            for service in &new_services {
                if services.contains_name(service.name()) || services.contains_id(service.id()) {
                    return Err(FoxgloveError::DuplicateService(service.name().to_string()));
                }
            }

            // Update the service map.
            for service in new_services {
                services.insert(service);
            }
        }

        // If we failed to generate any advertisements, don't send an empty message.
        if msg.services.is_empty() {
            return Ok(());
        }

        let clients = self.clients.get();
        for client in clients.iter().cloned() {
            for (name, id) in &new_names {
                tracing::debug!(
                    "Advertising service {name} with id {id} to client {}",
                    client.addr
                );
            }
            client.send_control_msg(&msg);
        }

        Ok(())
    }

    /// Removes services, and unadvertises them to all clients.
    ///
    /// Unrecognized service IDs are silently ignored.
    pub fn remove_services(&self, names: impl IntoIterator<Item = impl AsRef<str>>) {
        // Remove services from the map.
        let names = names.into_iter();
        let mut old_services = HashMap::with_capacity(names.size_hint().0);
        {
            let mut services = self.services.write();
            for name in names {
                if let Some(service) = services.remove_by_name(name) {
                    old_services.insert(service.id(), service.name().to_string());
                }
            }
        }
        if old_services.is_empty() {
            return;
        }

        // Prepare an unadvertisement.
        let msg = UnadvertiseServices::new(old_services.keys().map(|&id| id.into()));

        let clients = self.clients.get();
        for client in clients.iter().cloned() {
            for (id, name) in &old_services {
                tracing::debug!(
                    "Unadvertising service {name} with id {id} to client {}",
                    client.addr
                );
            }
            client.send_control_msg(&msg);
        }
    }

    // Looks up a service by ID.
    fn get_service(&self, id: ServiceId) -> Option<Arc<Service>> {
        self.services.read().get_by_id(id)
    }

    /// Sends a connection graph update to all clients.
    pub(crate) fn replace_connection_graph(
        &self,
        replacement_graph: ConnectionGraph,
    ) -> Result<(), FoxgloveError> {
        // Make sure that the server supports connection graph.
        if !self.capabilities.contains(&Capability::ConnectionGraph) {
            return Err(FoxgloveError::ConnectionGraphNotSupported);
        }

        // Hold the lock while sending to synchronize with subscribe and unsubscribe.
        let mut graph = self.connection_graph.lock();
        let msg = graph.update(replacement_graph);
        for client in self.clients.get().iter() {
            if graph.is_subscriber(client.id()) {
                client.send_control_msg(&msg);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum SendLossyResult {
    Sent,
    #[allow(dead_code)]
    SentLossy(usize),
    ExhaustedRetries,
}

/// Attempt to send a message on the channel.
///
/// If the channel is non-full, this function returns `SendLossyResult::Sent`.
///
/// If the channel is full, drop the oldest message and try again. If the send eventually succeeds
/// in this manner, this function returns `SendLossyResult::SentLossy(dropped)`. If the maximum
/// number of retries is reached, it returns `SendLossyResult::ExhaustedRetries`.
fn send_lossy(
    client_addr: &SocketAddr,
    tx: &flume::Sender<Message>,
    rx: &flume::Receiver<Message>,
    mut message: Message,
    retries: usize,
) -> SendLossyResult {
    // If the queue is full, drop the oldest message(s). We do this because the websocket
    // client is falling behind, and we either start dropping messages, or we'll end up
    // buffering until we run out of memory. There's no point in that because the client is
    // unlikely to catch up and be able to consume the messages.
    let mut dropped = 0;
    loop {
        match (dropped, tx.try_send(message)) {
            (0, Ok(_)) => return SendLossyResult::Sent,
            (dropped, Ok(_)) => {
                tracing::warn!(
                    "outbox for client {} full, dropped {dropped} messages",
                    client_addr
                );
                return SendLossyResult::SentLossy(dropped);
            }
            (_, Err(TrySendError::Disconnected(_))) => unreachable!("we're holding rx"),
            (_, Err(TrySendError::Full(rejected))) => {
                if dropped >= retries {
                    tracing::warn!(
                        "outbox for client {} full, dropping message after 10 attempts",
                        client_addr
                    );
                    return SendLossyResult::ExhaustedRetries;
                }
                message = rejected;
                let _ = rx.try_recv();
                dropped += 1
            }
        }
    }
}

impl Sink for ConnectedClient {
    fn id(&self) -> SinkId {
        self.sink_id
    }

    fn log(
        &self,
        channel: &RawChannel,
        msg: &[u8],
        metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        let subscriptions = self.subscriptions.lock();
        let Some(subscription_id) = subscriptions.get_by_left(&channel.id()).copied() else {
            return Ok(());
        };

        let message = MessageData::new(subscription_id.into(), metadata.log_time, msg);
        self.send_data_lossy(&message, MAX_SEND_RETRIES);
        Ok(())
    }

    /// Server has an available channel. Advertise to all clients.
    fn add_channel(&self, channel: &Arc<RawChannel>) -> bool {
        self.advertise_channel(channel);
        false
    }

    /// A channel is being removed. Unadvertise to all clients.
    fn remove_channel(&self, channel: &RawChannel) {
        self.unadvertise_channel(channel.id());
    }

    /// Clients maintain subscriptions dynamically.
    fn auto_subscribe(&self) -> bool {
        false
    }
}

pub(crate) fn create_server(ctx: &Arc<Context>, opts: ServerOptions) -> Arc<Server> {
    Arc::new_cyclic(|weak_self| Server::new(weak_self.clone(), ctx, opts))
}

// Spawn a new task for each incoming connection
async fn handle_connections(server: Arc<Server>, listener: TcpListener) {
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(server.clone().handle_connection(stream, addr));
    }
}

/// Add the subprotocol header to the response if the client requested it. If the client requests
/// subprotocols which don't contain ours, or does not include the expected header, return a 400.
async fn do_handshake(stream: TcpStream) -> Result<WebSocketStream<TcpStream>, tungstenite::Error> {
    tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &server::Request, mut res: server::Response| {
            let protocol_headers = req.headers().get_all("sec-websocket-protocol");
            for header in &protocol_headers {
                if header
                    .to_str()
                    .unwrap_or_default()
                    .split(',')
                    .any(|v| v.trim() == SUBPROTOCOL)
                {
                    res.headers_mut().insert(
                        "sec-websocket-protocol",
                        HeaderValue::from_static(SUBPROTOCOL),
                    );
                    return Ok(res);
                }
            }

            let resp = server::Response::builder()
                .status(400)
                .body(Some(
                    "Missing expected sec-websocket-protocol header".into(),
                ))
                .unwrap();

            Err(resp)
        },
    )
    .await
}
