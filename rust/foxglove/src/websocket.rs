//! Websocket functionality

use std::collections::hash_map::Entry;
use std::collections::HashSet;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32};
use std::sync::Weak;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use bimap::BiHashMap;
use bytes::{BufMut, Bytes, BytesMut};
use flume::TrySendError;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde::Serialize;
use thiserror::Error;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::{self, handshake::server, http::HeaderValue, Message};
use tokio_tungstenite::WebSocketStream;
use tokio_util::sync::CancellationToken;

use crate::channel::ChannelId;
use crate::cow_vec::CowVec;
use crate::{get_runtime_handle, Channel, Context, FoxgloveError, Metadata, Sink, SinkId};

mod fetch_asset;
pub use fetch_asset::{AssetHandler, AssetResponder};
pub(crate) use fetch_asset::{AsyncAssetHandlerFn, BlockingAssetHandlerFn};
mod connection_graph;
mod protocol;
pub use protocol::client::{ClientChannel, ClientChannelId};
pub(crate) use protocol::client::{ClientMessage, Subscription, SubscriptionId};
pub use protocol::server::{Parameter, ParameterType, ParameterValue, Status, StatusLevel};
mod semaphore;
pub mod service;
pub use connection_graph::ConnectionGraph;
pub(crate) use semaphore::{Semaphore, SemaphoreGuard};
use service::{CallId, Service, ServiceId, ServiceMap};
#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "unstable"))]
mod unstable_tests;

/// A capability that a websocket server can support.
#[derive(Debug, Serialize, Eq, PartialEq, Hash, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
    /// Allow clients to get & set parameters, and subscribe to updates.
    Parameters,
    /// Inform clients about the latest server time.
    ///
    /// This allows accelerated, slowed, or stepped control over the progress of time. If the
    /// server publishes time data, then timestamps of published messages must originate from the
    /// same time source.
    #[cfg(feature = "unstable")]
    Time,
    /// Allow clients to call services.
    Services,
    /// Allow clients to request assets. If you supply an asset handler to the server, this
    /// capability will be advertised automatically.
    Assets,
    /// Allow clients to subscribe and make connection graph updates
    ConnectionGraph,
}

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
    subscribed_parameters: parking_lot::Mutex<HashSet<String>>,
    /// Encodings server can accept from clients. Ignored unless the "clientPublish" capability is set.
    supported_encodings: HashSet<String>,
    /// The current connection graph, unused unless the "connectionGraph" capability is set.
    /// see https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#connection-graph-update
    connection_graph: parking_lot::Mutex<ConnectionGraph>,
    /// The number of clients subscribed to the connection graph
    /// This is a mutex, not an atomic, as it's used to synchronize calls to on_connection_graph_subscribe/unsubscribe
    connection_graph_subscriber_count: parking_lot::Mutex<u32>,
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
    /// Callback invoked when a client unsubscribes from a channel.
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
    channels: parking_lot::RwLock<HashMap<ChannelId, Arc<Channel>>>,
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
    /// Parameters subscribed to by this client
    parameter_subscriptions: parking_lot::Mutex<HashSet<String>>,
    /// Optional callback handler for a server implementation
    server_listener: Option<Arc<dyn ServerListener>>,
    server: Weak<Server>,
    /// The cancellation_token is used by the server to disconnect the client.
    /// It's cancelled when the client's control plane queue fills up (slow client).
    cancellation_token: CancellationToken,
    /// Whether this client is subscribed to the connection graph
    /// This is updated only with the connection_graph mutex held in on_connection_graph_subscribe and unsubscribe.
    /// It's read with the connection_graph mutex held, when sending connection graph updates to clients.
    subscribed_to_connection_graph: AtomicBool,
}

impl ConnectedClient {
    fn arc(&self) -> Arc<Self> {
        self.weak_self
            .upgrade()
            .expect("client cannot be dropped while in use")
    }

    fn is_subscribed_to_connection_graph(&self) -> bool {
        self.subscribed_to_connection_graph.load(Acquire)
    }

    /// Handle a text or binary message sent from the client.
    ///
    /// Standard protocol messages (such as Close) should be handled upstream.
    fn handle_message(&self, message: Message) {
        let parse_result = match message {
            Message::Text(bytes) => ClientMessage::parse_json(bytes.as_str()),
            Message::Binary(bytes) => match ClientMessage::parse_binary(bytes) {
                Err(e) => Err(e),
                Ok(Some(msg)) => Ok(msg),
                Ok(None) => {
                    tracing::debug!("Received empty binary message from {}", self.addr);
                    return;
                }
            },
            _ => {
                tracing::debug!("Unhandled websocket message: {message:?}");
                return;
            }
        };
        let msg = match parse_result {
            Ok(msg) => msg,
            Err(err) => {
                tracing::error!("Invalid message from {}: {err}", self.addr);
                self.send_error(format!("Invalid message: {err}"));
                return;
            }
        };
        let Some(server) = self.server.upgrade() else {
            return;
        };

        match msg {
            ClientMessage::Subscribe(msg) => self.on_subscribe(msg.subscriptions),
            ClientMessage::Unsubscribe(msg) => self.on_unsubscribe(msg.subscription_ids),
            ClientMessage::Advertise(msg) => self.on_advertise(server, msg.channels),
            ClientMessage::Unadvertise(msg) => self.on_unadvertise(msg.channel_ids),
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
    fn send_data_lossy(&self, message: Message, retries: usize) -> SendLossyResult {
        send_lossy(
            &self.addr,
            &self.data_plane_tx,
            &self.data_plane_rx,
            message,
            retries,
        )
    }

    /// Send the message on the control plane, disconnecting the client if the channel is full.
    fn send_control_msg(&self, message: Message) -> bool {
        if let Err(TrySendError::Full(_)) = self.control_plane_tx.try_send(message) {
            self.cancellation_token.cancel();
            return false;
        }
        true
    }

    async fn on_disconnect(&self, server: &Arc<Server>) {
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

        // If we track paramter subscriptions, unsubscribe this clients subscriptions
        // and notify the handler, if necessary
        if !server.capabilities.contains(&Capability::Parameters) || self.server_listener.is_none()
        {
            return;
        }

        // Remove all subscriptions from the server subscriptions.
        // First take the server-wide lock.
        let mut all_subscriptions = server.subscribed_parameters.lock();

        // Remove the parameter subscriptions for this client,
        // and filter out any we weren't subscribed to.
        let mut client_subscriptions = self.parameter_subscriptions.lock();
        let client_subscriptions = std::mem::take(&mut *client_subscriptions);
        let mut unsubscribed_parameters =
            server.parameters_without_subscription(client_subscriptions.into_iter().collect());
        if unsubscribed_parameters.is_empty() {
            return;
        }

        unsubscribed_parameters.retain(|name| all_subscriptions.remove(name));
        if let Some(handler) = self.server_listener.as_ref() {
            handler.on_parameters_unsubscribe(unsubscribed_parameters);
        }
    }

    fn on_message_data(&self, message: protocol::client::ClientMessageData) {
        let channel_id = message.channel_id;
        let payload = message.payload;
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

    fn on_unadvertise(&self, mut channel_ids: Vec<ClientChannelId>) {
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

    fn on_advertise(&self, server: Arc<Server>, channels: Vec<ClientChannel>) {
        if !server.capabilities.contains(&Capability::ClientPublish) {
            self.send_error("Server does not support clientPublish capability".to_string());
            return;
        }

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

    fn on_unsubscribe(&self, subscription_ids: Vec<SubscriptionId>) {
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

    fn on_subscribe(&self, mut subscriptions: Vec<Subscription>) {
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
            let request_id = request_id.as_deref();
            let parameters = handler.on_get_parameters(Client::new(self), param_names, request_id);
            let message = protocol::server::parameters_json(&parameters, request_id);
            let _ = self.control_plane_tx.try_send(Message::text(message));
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
            let request_id = request_id.as_deref();
            let updated_parameters =
                handler.on_set_parameters(Client::new(self), parameters, request_id);
            // Send all the updated_parameters back to the client if request_id is provided.
            // This is the behavior of the reference Python server implementation.
            if request_id.is_some() {
                let message = protocol::server::parameters_json(&updated_parameters, request_id);
                self.send_control_msg(Message::text(message));
            }
            updated_parameters
        } else {
            // This differs from the Python legacy ws-protocol implementation in that here we notify
            // subscribers about the parameters even if there's no ServerListener configured.
            // This seems to be a more sensible default.
            parameters
        };
        server.publish_parameter_values(updated_parameters);
    }

    fn update_parameters(&self, parameters: &[Parameter]) {
        // Hold the lock for as short a time as possible
        let subscribed_parameters: Vec<Parameter> = {
            let subscribed_parameters = self.parameter_subscriptions.lock();
            // Filter parameters to only send the ones the client is subscribed to
            parameters
                .iter()
                .filter(|p| subscribed_parameters.contains(&p.name))
                .cloned()
                .collect()
        };
        if subscribed_parameters.is_empty() {
            return;
        }
        let message = protocol::server::parameters_json(&subscribed_parameters, None);
        self.send_control_msg(Message::text(message));
    }

    fn on_parameters_subscribe(&self, server: Arc<Server>, param_names: Vec<String>) {
        if !server.capabilities.contains(&Capability::Parameters) {
            self.send_error("Server does not support parametersSubscribe capability".to_string());
            return;
        }

        // We hold the server lock here the entire time to serialize
        // calls to subscribe and unsubscribe, otherwise there are all
        // kinds of race conditions here where handlers get invoked in
        // an order different than the order the events were applied,
        // leading to the listener thinking it has no subscribers to a
        // parameter when it actually does or visa versa.
        let mut new_param_subscriptions = Vec::with_capacity(
            self.server_listener
                .as_ref()
                .map(|_| param_names.len())
                .unwrap_or_default(),
        );
        let mut all_subscriptions = server.subscribed_parameters.lock();

        // Get the list of which subscriptions are new to the server (first time subscriptions)
        if self.server_listener.is_some() {
            for name in &param_names {
                if all_subscriptions.insert(name.clone()) {
                    new_param_subscriptions.push(name.clone());
                }
            }
        }

        {
            // Track the client's own subscriptions
            let mut client_subscriptions = self.parameter_subscriptions.lock();
            client_subscriptions.extend(param_names);
        }

        if new_param_subscriptions.is_empty() {
            return;
        }

        if let Some(handler) = self.server_listener.as_ref() {
            // We hold the server subscribed_parameters mutex across the call to the handler
            // to synchrnize with other
            handler.on_parameters_subscribe(new_param_subscriptions);
        }
    }

    fn on_parameters_unsubscribe(&self, server: Arc<Server>, mut param_names: Vec<String>) {
        if !server.capabilities.contains(&Capability::Parameters) {
            self.send_error("Server does not support parametersSubscribe capability".to_string());
            return;
        }

        // Like in subscribe, we first take the server-wide lock.
        let mut all_subscriptions = server.subscribed_parameters.lock();

        {
            // Remove the parameter subscriptions for this client,
            // and filter out any we weren't subscribed to.
            let mut client_subscriptions = self.parameter_subscriptions.lock();
            param_names.retain(|name| client_subscriptions.remove(name));
        }

        if param_names.is_empty() {
            // We didn't remove any subscriptions
            return;
        }

        let Some(handler) = self.server_listener.as_ref() else {
            return;
        };

        let mut unsubscribed_parameters = server.parameters_without_subscription(param_names);
        // Remove the unsubscribed parameters from the server's list of subscribed parameters
        unsubscribed_parameters.retain(|name| all_subscriptions.remove(name));
        // We have to hold the lock while calling the handler because we need
        // to synchronize this with other calls to on_parameters_subscribe and on_parameters_unsubscribe
        handler.on_parameters_unsubscribe(unsubscribed_parameters);
    }

    fn on_service_call(&self, req: protocol::client::ServiceCallRequest) {
        let Some(server) = self.server.upgrade() else {
            return;
        };

        // We have a response channel if and only if the server supports services.
        let service_id = req.service_id;
        let call_id = req.call_id;
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
            .unwrap_or_else(|| server.supported_encodings.contains(&req.encoding))
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
        let request =
            service::Request::new(service.clone(), self.id, call_id, req.encoding, req.payload);

        // Invoke the handler.
        service.call(request, responder);
    }

    /// Sends a service call failure message to the client with the provided message.
    fn send_service_call_failure(&self, service_id: ServiceId, call_id: CallId, message: &str) {
        let msg = Message::text(protocol::server::service_call_failure(
            service_id, call_id, message,
        ));
        self.send_control_msg(msg);
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
        if !server.capabilities.contains(&Capability::ConnectionGraph) {
            self.send_error("Server does not support connection graph capability".to_string());
            return;
        }

        let is_subscribed = self.subscribed_to_connection_graph.load(Acquire);
        if is_subscribed {
            tracing::debug!(
                "Client {} is already subscribed to connection graph updates",
                self.addr
            );
            return;
        }

        {
            let mut subscriber_count = server.connection_graph_subscriber_count.lock();
            let is_first_subscriber = *subscriber_count == 0;
            *subscriber_count += 1;

            // We hold the lock over the call to the listener so that subscribe and unsubscribe
            // calls are correctly ordered relative to each other.
            if is_first_subscriber {
                if let Some(listener) = server.listener.as_ref() {
                    listener.on_connection_graph_subscribe();
                }
            }
        }

        // We hold the connection_graph lock over updating self.subscribed_to_connection_graph
        // and sending the initial update message, so it's synchronized with unsubscribe and
        // with server.connection_graph_update.
        let mut connection_graph = server.connection_graph.lock();
        // Take the graph and replace it with an empty default
        let current_graph = std::mem::take(&mut *connection_graph);
        // Update the empty default with the current graph, setting it back to where it was,
        // and generating the full diff in the process.
        let json_diff = connection_graph.update(current_graph);
        // Send the full diff to the client as the starting state
        self.send_control_msg(Message::text(json_diff));
        self.subscribed_to_connection_graph.store(true, Release);
    }

    fn on_connection_graph_unsubscribe(&self, server: Arc<Server>) {
        if !server.capabilities.contains(&Capability::ConnectionGraph) {
            self.send_error("Server does not support connection graph capability".to_string());
            return;
        }

        let is_subscribed = self.is_subscribed_to_connection_graph();
        if !is_subscribed {
            self.send_error("Client is not subscribed to connection graph updates".to_string());
            return;
        }

        {
            let mut subscriber_count = server.connection_graph_subscriber_count.lock();
            *subscriber_count -= 1;

            if *subscriber_count == 0 {
                if let Some(listener) = server.listener.as_ref() {
                    listener.on_connection_graph_unsubscribe();
                }
            }
        }

        // Acquire the lock to sychronize with subscribe and with server.connection_graph_update.
        let _guard = server.connection_graph.lock();
        self.subscribed_to_connection_graph.store(false, Release);
    }

    /// Send an ad hoc error status message to the client, with the given message.
    fn send_error(&self, message: String) {
        tracing::debug!("Sending error to client {}: {}", self.addr, message);
        self.send_status(Status::new(StatusLevel::Error, message));
    }

    /// Send an ad hoc warning status message to the client, with the given message.
    fn send_warning(&self, message: String) {
        tracing::debug!("Sending warning to client {}: {}", self.addr, message);
        self.send_status(Status::new(StatusLevel::Warning, message));
    }

    /// Send a status message to the client.
    fn send_status(&self, status: Status) {
        let message = Message::text(serde_json::to_string(&status).unwrap());
        match status.level {
            StatusLevel::Info => {
                self.send_data_lossy(message, MAX_SEND_RETRIES);
            }
            _ => {
                self.send_control_msg(message);
            }
        }
    }

    /// Send a fetch asset error to the client.
    fn send_asset_error(&self, error: &str, request_id: u32) {
        // https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset-response
        let mut buf = Vec::with_capacity(10 + error.len());
        buf.put_u8(protocol::server::BinaryOpcode::FetchAssetResponse as u8);
        buf.put_u32_le(request_id);
        buf.put_u8(1); // 1 for error
        buf.put_u32_le(error.len() as u32);
        buf.put(error.as_bytes());
        let message = Message::binary(buf);
        self.send_control_msg(message);
    }

    /// Send a fetch asset response to the client.
    fn send_asset_response(&self, response: &[u8], request_id: u32) {
        // https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#fetch-asset-response
        let mut buf = Vec::with_capacity(10 + response.len());
        buf.put_u8(protocol::server::BinaryOpcode::FetchAssetResponse as u8);
        buf.put_u32_le(request_id);
        buf.put_u8(0); // 0 for success
        buf.put_u32_le(0); // error length, 0 for no error
        buf.put(response);
        let message = Message::binary(buf);
        self.send_control_msg(message);
    }

    /// Advertises a channel to the client.
    fn advertise_channel(&self, channel: &Arc<Channel>) {
        let message = match protocol::server::advertisement(channel) {
            Ok(message) => message,
            Err(FoxgloveError::SchemaRequired) => {
                tracing::error!(
                    "Ignoring advertise channel for {} because a schema is required",
                    channel.topic()
                );
                return;
            }
            Err(err) => {
                tracing::error!("Error advertising channel to client: {err}");
                return;
            }
        };

        self.channels.write().insert(channel.id(), channel.clone());

        if self.send_control_msg(Message::text(message.clone())) {
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

        let message = protocol::server::unadvertise(channel_id);
        if self.send_control_msg(Message::text(message.clone())) {
            tracing::debug!(
                "Unadvertised channel with id {} to client {}",
                channel_id,
                self.addr
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
            subscribed_parameters: parking_lot::Mutex::new(HashSet::new()),
            capabilities,
            supported_encodings,
            connection_graph: parking_lot::Mutex::new(ConnectionGraph::new()),
            connection_graph_subscriber_count: parking_lot::Mutex::new(0),
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

    /// Filter param_names to just those with no subscribers
    fn parameters_without_subscription(&self, mut param_names: Vec<String>) -> Vec<String> {
        let clients = self.clients.get();
        for client in clients.iter() {
            let subscribed_parameters = client.parameter_subscriptions.lock();
            // Remove any parameters that are already subscribed to by this client
            param_names.retain(|name| !subscribed_parameters.contains(name));
        }
        // The remaining parameters are those with no subscribers
        param_names
    }

    /// Publish the current timestamp to all clients.
    #[cfg(feature = "unstable")]
    pub fn broadcast_time(&self, timestamp_nanos: u64) {
        if !self.capabilities.contains(&Capability::Time) {
            tracing::error!("Server does not support time capability");
            return;
        }

        // https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#time
        let mut buf = BytesMut::with_capacity(9);
        buf.put_u8(protocol::server::BinaryOpcode::TimeData as u8);
        buf.put_u64_le(timestamp_nanos);
        let message = Message::binary(buf);

        let clients = self.clients.get();
        for client in clients.iter() {
            client.send_control_msg(message.clone());
        }
    }

    /// Publish parameter values to all subscribed clients.
    pub fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        if !self.capabilities.contains(&Capability::Parameters) {
            tracing::error!("Server does not support parameters capability");
            return;
        }

        let clients = self.clients.get();
        for client in clients.iter() {
            client.update_parameters(&parameters);
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
        let remove = protocol::server::RemoveStatus { status_ids };
        let message = Message::text(serde_json::to_string(&remove).unwrap());
        let clients = self.clients.get();
        for client in clients.iter() {
            client.send_control_msg(message.clone());
        }
    }

    /// Sets a new session ID and notifies all clients, causing them to reset their state.
    /// If no session ID is provided, generates a new one based on the current timestamp.
    pub fn clear_session(&self, new_session_id: Option<String>) {
        *self.session_id.write() = new_session_id.unwrap_or_else(Self::generate_session_id);

        let info_message = protocol::server::server_info(
            &self.session_id.read(),
            &self.name,
            &self.capabilities,
            &self.supported_encodings,
        );

        let message = Message::text(info_message);
        let clients = self.clients.get();
        for client in clients.iter() {
            client.send_control_msg(message.clone());
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

        let info_message = protocol::server::server_info(
            &self.session_id.read(),
            &self.name,
            &self.capabilities,
            &self.supported_encodings,
        );
        if let Err(err) = ws_sender.send(Message::text(info_message)).await {
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
            parameter_subscriptions: parking_lot::Mutex::new(HashSet::new()),
            server_listener: self.listener.clone(),
            server: self.weak_self.clone(),
            cancellation_token: cancellation_token.clone(),
            subscribed_to_connection_graph: AtomicBool::new(false),
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
        new_client.on_disconnect(&self).await;
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
        if !services.is_empty() {
            let msg = Message::text(protocol::server::advertise_services(
                services.iter().map(|s| s.as_ref()),
            ));
            if client.send_control_msg(msg) {
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

        let mut new_names = HashMap::with_capacity(new_services.len());
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
        }

        // Prepare an advertisement.
        let msg = Message::text(protocol::server::advertise_services(&new_services));

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

        // Send advertisements.
        let clients = self.clients.get();
        for client in clients.iter().cloned() {
            for (name, id) in &new_names {
                tracing::debug!(
                    "Advertising service {name} with id {id} to client {}",
                    client.addr
                );
            }
            client.send_control_msg(msg.clone());
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
        let msg = Message::text(protocol::server::unadvertise_services(
            &old_services.keys().copied().collect::<Vec<_>>(),
        ));

        let clients = self.clients.get();
        for client in clients.iter().cloned() {
            for (id, name) in &old_services {
                tracing::debug!(
                    "Unadvertising service {name} with id {id} to client {}",
                    client.addr
                );
            }
            client.send_control_msg(msg.clone());
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
        let mut connection_graph = self.connection_graph.lock();
        let json_diff = connection_graph.update(replacement_graph);
        let msg = Message::text(json_diff);
        for client in self.clients.get().iter() {
            if client.is_subscribed_to_connection_graph() {
                client.send_control_msg(msg.clone());
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

    fn log(&self, channel: &Channel, msg: &[u8], metadata: &Metadata) -> Result<(), FoxgloveError> {
        let subscriptions = self.subscriptions.lock();
        let Some(subscription_id) = subscriptions.get_by_left(&channel.id()).copied() else {
            return Ok(());
        };

        // https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#message-data
        let header_size: usize = 1 + 4 + 8;
        let mut buf = BytesMut::with_capacity(header_size + msg.len());
        buf.put_u8(protocol::server::BinaryOpcode::MessageData as u8);
        buf.put_u32_le(subscription_id.into());
        buf.put_u64_le(metadata.log_time);
        buf.put_slice(msg);

        let message = Message::binary(buf);

        self.send_data_lossy(message, MAX_SEND_RETRIES);
        Ok(())
    }

    /// Server has an available channel. Advertise to all clients.
    fn add_channel(&self, channel: &Arc<Channel>) -> bool {
        self.advertise_channel(channel);
        false
    }

    /// A channel is being removed. Unadvertise to all clients.
    fn remove_channel(&self, channel: &Channel) {
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

/// Add the subprotocol header to the response if the client requested one we support.
/// If the client doesn't support our protocol, do not include the protocol header in the response;
/// the client must fail the connection. [WebSocket RFC](https://www.rfc-editor.org/rfc/rfc6455#section-4)
async fn do_handshake(stream: TcpStream) -> Result<WebSocketStream<TcpStream>, tungstenite::Error> {
    tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &server::Request, mut res: server::Response| {
            let all_headers = req.headers().get_all("sec-websocket-protocol");
            if all_headers.iter().any(|h| {
                (*h).to_str()
                    .unwrap_or_default()
                    .split(',')
                    .any(|s| s.trim() == SUBPROTOCOL)
            }) {
                res.headers_mut().insert(
                    "sec-websocket-protocol",
                    HeaderValue::from_static(SUBPROTOCOL),
                );
            };
            Ok(res)
        },
    )
    .await
}
