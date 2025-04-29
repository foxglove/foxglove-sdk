use std::collections::hash_map::Entry;
use std::sync::Weak;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use bimap::BiHashMap;
use flume::TrySendError;
use send_lossy::SendLossyResult;
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::{ChannelId, Context, FoxgloveError, Metadata, RawChannel, Sink, SinkId};

use self::ws_protocol::server::{
    FetchAssetResponse, ParameterValues, ServiceCallFailure, Unadvertise,
};

use super::semaphore::Semaphore;
use super::server::Server;
use super::service::{self, CallId, ServiceId};
use super::subscription::{Subscription, SubscriptionId};
use super::ws_protocol::client::ClientMessage;
use super::ws_protocol::server::MessageData;
use super::ws_protocol::{self, ParseError};
use super::{
    advertise, AssetResponder, Capability, Client, ClientChannel, ClientChannelId, ClientId,
    Parameter, Status, StatusLevel,
};

mod poller;
mod send_lossy;

use poller::Poller;

const MAX_SEND_RETRIES: usize = 10;
const ADVERTISE_CHANNEL_BATCH_SIZE: usize = 100;
const DEFAULT_SERVICE_CALLS_PER_CLIENT: usize = 32;
const DEFAULT_FETCH_ASSET_CALLS_PER_CLIENT: usize = 32;

/// A reason for shutting down a client connection.
#[derive(Debug, Clone, Copy)]
pub(super) enum ShutdownReason {
    /// The client disconnected.
    ClientDisconnected,
    /// The server has been stopped.
    ServerStopped,
    /// The control plane queue overflowed, and the client must be disconnected.
    ControlPlaneQueueFull,
}

/// A connected client session with the websocket server.
pub(super) struct ConnectedClient {
    id: ClientId,
    addr: SocketAddr,
    weak_self: Weak<Self>,
    sink_id: SinkId,
    context: Weak<Context>,
    poller: parking_lot::Mutex<Option<Poller>>,
    /// A cache of channels for `on_subscribe` and `on_unsubscribe` callbacks.
    channels: parking_lot::RwLock<HashMap<ChannelId, Arc<RawChannel>>>,
    data_plane_tx: flume::Sender<Message>,
    data_plane_rx: flume::Receiver<Message>,
    control_plane_tx: flume::Sender<Message>,
    service_call_sem: Semaphore,
    fetch_asset_sem: Semaphore,
    /// Subscriptions from this client
    subscriptions: parking_lot::Mutex<BiHashMap<ChannelId, SubscriptionId>>,
    /// Channels advertised by this client
    advertised_channels: parking_lot::Mutex<HashMap<ClientChannelId, Arc<ClientChannel>>>,
    server: Weak<Server>,
    shutdown_tx: parking_lot::Mutex<Option<oneshot::Sender<ShutdownReason>>>,
}

impl std::fmt::Debug for ConnectedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("id", &self.id)
            .field("address", &self.addr)
            .finish()
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

    fn add_channels(&self, channels: &[&Arc<RawChannel>]) -> Option<Vec<ChannelId>> {
        for channels in channels.chunks(ADVERTISE_CHANNEL_BATCH_SIZE) {
            self.advertise_channels(channels);
        }
        // Clients subscribe asynchronously.
        None
    }

    fn remove_channel(&self, channel: &RawChannel) {
        self.unadvertise_channel(channel.id());
    }

    fn auto_subscribe(&self) -> bool {
        // Clients maintain subscriptions dynamically.
        false
    }
}

impl ConnectedClient {
    pub fn new(
        context: &Weak<Context>,
        server: &Weak<Server>,
        websocket: WebSocketStream<TcpStream>,
        addr: SocketAddr,
        message_backlog_size: usize,
    ) -> Arc<Self> {
        let (data_plane_tx, data_plane_rx) = flume::bounded(message_backlog_size);
        let (control_plane_tx, control_plane_rx) = flume::bounded(message_backlog_size);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        Arc::new_cyclic(|weak_self| Self {
            id: ClientId::next(),
            addr,
            weak_self: weak_self.clone(),
            sink_id: SinkId::next(),
            context: context.clone(),
            poller: parking_lot::Mutex::new(Some(Poller::new(
                websocket,
                data_plane_rx.clone(),
                control_plane_rx,
                shutdown_rx,
            ))),
            channels: parking_lot::RwLock::default(),
            data_plane_tx,
            data_plane_rx,
            control_plane_tx,
            service_call_sem: Semaphore::new(DEFAULT_SERVICE_CALLS_PER_CLIENT),
            fetch_asset_sem: Semaphore::new(DEFAULT_FETCH_ASSET_CALLS_PER_CLIENT),
            subscriptions: parking_lot::Mutex::default(),
            advertised_channels: parking_lot::Mutex::default(),
            server: server.clone(),
            shutdown_tx: parking_lot::Mutex::new(Some(shutdown_tx)),
        })
    }

    pub fn id(&self) -> ClientId {
        self.id
    }

    pub fn sink_id(&self) -> SinkId {
        self.sink_id
    }

    fn arc(&self) -> Arc<Self> {
        self.weak_self
            .upgrade()
            .expect("client cannot be dropped while in use")
    }

    pub fn weak(&self) -> &Weak<Self> {
        &self.weak_self
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Runs the client's poll loop to completion.
    ///
    /// The poll loop may exit either due to the client closing the connection, or due to an
    /// internal call to [`ConnectedClient::shutdown`].
    ///
    /// Panics if called more than once.
    pub async fn run(&self) {
        let poller = self.poller.lock().take().expect("only call run once");
        poller.run(self).await;
    }

    /// Shuts down the connection by signalling the [`Poller`] to exit.
    pub fn shutdown(&self, reason: ShutdownReason) {
        if let Some(shutdown_tx) = self.shutdown_tx.lock().take() {
            shutdown_tx.send(reason).ok();
        }
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
            ClientMessage::Subscribe(msg) => self.on_subscribe(server, msg),
            ClientMessage::Unsubscribe(msg) => self.on_unsubscribe(msg),
            ClientMessage::Advertise(msg) => self.on_advertise(server, msg),
            ClientMessage::Unadvertise(msg) => self.on_unadvertise(server, msg),
            ClientMessage::MessageData(msg) => self.on_message_data(server, msg),
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
            ClientMessage::ServiceCallRequest(msg) => self.on_service_call(server, msg),
            ClientMessage::FetchAsset(msg) => self.on_fetch_asset(server, msg.uri, msg.request_id),
            ClientMessage::SubscribeConnectionGraph => self.on_connection_graph_subscribe(server),
            ClientMessage::UnsubscribeConnectionGraph => {
                self.on_connection_graph_unsubscribe(server)
            }
        }
    }

    /// Send the message on the data plane, dropping up to retries older messages to make room, if necessary.
    fn send_data_lossy(&self, message: impl Into<Message>, retries: usize) -> SendLossyResult {
        send_lossy::send_lossy(
            &self.addr,
            &self.data_plane_tx,
            &self.data_plane_rx,
            message.into(),
            retries,
        )
    }

    /// Send the message on the control plane, disconnecting the client if the channel is full.
    pub fn send_control_msg(&self, message: impl Into<Message>) -> bool {
        if let Err(TrySendError::Full(_)) = self.control_plane_tx.try_send(message.into()) {
            self.shutdown(ShutdownReason::ControlPlaneQueueFull);
            false
        } else {
            true
        }
    }

    /// Called when the server finally drops the connection.
    pub fn on_disconnect(&self) {
        let channel_ids = self.subscriptions.lock().left_values().copied().collect();
        self.unsubscribe_channel_ids(channel_ids);
    }

    fn on_message_data(&self, server: Arc<Server>, message: ws_protocol::client::MessageData) {
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
        if let Some(handler) = server.listener() {
            handler.on_message_data(Client::new(self), &client_channel, &payload);
        }
    }

    fn on_unadvertise(&self, server: Arc<Server>, message: ws_protocol::client::Unadvertise) {
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
        if let Some(handler) = server.listener() {
            for client_channel in client_channels {
                handler.on_client_unadvertise(Client::new(self), &client_channel);
            }
        }
    }

    fn on_advertise(&self, server: Arc<Server>, message: ws_protocol::client::Advertise) {
        if !server.has_capability(Capability::ClientPublish) {
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
            if let Some(handler) = server.listener() {
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

    fn on_subscribe(&self, server: Arc<Server>, message: ws_protocol::client::Subscribe) {
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

            if let Some(handler) = server.listener() {
                handler.on_subscribe(Client::new(self), channel.as_ref().into());
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
        if !server.has_capability(Capability::Parameters) {
            self.send_error("Server does not support parameters capability".to_string());
            return;
        }

        if let Some(handler) = server.listener() {
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
        if !server.has_capability(Capability::Parameters) {
            self.send_error("Server does not support parameters capability".to_string());
            return;
        }

        let updated_parameters = if let Some(handler) = server.listener() {
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

    pub fn update_parameters(&self, parameters: Vec<Parameter>) {
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

    fn on_service_call(&self, server: Arc<Server>, req: ws_protocol::client::ServiceCallRequest) {
        // We have a response channel if and only if the server supports services.
        let service_id = ServiceId::new(req.service_id);
        let call_id = CallId::new(req.call_id);
        if !server.has_capability(Capability::Services) {
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
            .unwrap_or_else(|| server.supports_encoding(&req.encoding))
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
        if !server.has_capability(Capability::Assets) {
            self.send_error("Server does not support assets capability".to_string());
            return;
        }

        let Some(guard) = self.fetch_asset_sem.try_acquire() else {
            self.send_asset_error("Too many concurrent fetch asset requests", request_id);
            return;
        };

        if let Some(handler) = server.fetch_asset_handler() {
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
        if !server.has_capability(Capability::ConnectionGraph) {
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
    pub fn send_status(&self, status: Status) {
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
    pub fn send_asset_error(&self, error: &str, request_id: u32) {
        self.send_control_msg(&FetchAssetResponse::error_message(request_id, error));
    }

    /// Send a fetch asset response to the client.
    pub fn send_asset_response(&self, response: &[u8], request_id: u32) {
        self.send_control_msg(&FetchAssetResponse::asset_data(request_id, response));
    }

    /// Advertises a channel to the client.
    fn advertise_channels(&self, channels: &[&Arc<RawChannel>]) {
        let message = advertise::advertise_channels(channels.iter().copied());
        if message.channels.is_empty() {
            return;
        }

        self.channels
            .write()
            .extend(channels.iter().map(|&c| (c.id(), c.clone())));

        if self.send_control_msg(&message) {
            for channel in channels {
                tracing::debug!(
                    "Advertised channel {} with id {} to client {}",
                    channel.topic(),
                    channel.id(),
                    self.addr
                );
            }
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
        let server = self.server.upgrade();
        let Some(handler) = server.as_ref().and_then(|s| s.listener()) else {
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
            handler.on_unsubscribe(Client::new(self), channel.as_ref().into());
        }
    }
}
