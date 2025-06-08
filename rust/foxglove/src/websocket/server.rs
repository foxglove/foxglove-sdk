use std::collections::hash_map::Entry;
use std::collections::HashSet;
use std::sync::Weak;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use futures_util::SinkExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::task::{JoinError, JoinSet};
use tokio::time::MissedTickBehavior;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use crate::library_version::get_library_version;
use crate::websocket::connected_client::ShutdownReason;
use crate::{Context, FoxgloveError};

use super::connected_client::ConnectedClient;
use super::cow_vec::CowVec;
use super::service::{Service, ServiceId, ServiceMap};
use super::ws_protocol::server::{
    AdvertiseServices, RemoveStatus, ServerInfo, UnadvertiseServices,
};
use super::{
    advertise, handshake, AssetHandler, Capability, ClientId, ConnectionGraph, Parameter,
    ServerListener, Status,
};

// Queue up to 1024 messages per connected client before dropping messages
// Can be overridden by ServerOptions::message_backlog_size.
const DEFAULT_MESSAGE_BACKLOG_SIZE: usize = 1024;

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

/// Processes a task result, warning about panics.
fn process_task_result(result: Result<(), JoinError>) {
    match result {
        Err(e) if e.is_panic() => tracing::warn!("{e}"),
        _ => (),
    }
}

/// A handle that can be used to wait for the server to shut down.
#[must_use]
#[derive(Debug)]
pub struct ShutdownHandle {
    runtime: Handle,
    tasks: JoinSet<()>,
}
impl ShutdownHandle {
    /// Returns a new shutdown handle.
    fn new(runtime: Handle, tasks: JoinSet<()>) -> Self {
        Self { runtime, tasks }
    }

    /// Detaches all remaining tasks to complete in the background.
    pub fn detach_all(mut self) {
        self.tasks.detach_all();
    }

    /// Waits for all server tasks to stop.
    async fn wait_inner(&mut self) {
        while let Some(result) = self.tasks.join_next().await {
            process_task_result(result);
        }
        tracing::info!("Shutdown complete");
    }

    /// Waits for all server tasks to stop.
    pub async fn wait(mut self) {
        self.wait_inner().await;
    }

    /// Waits for all server tasks to stop from a blocking context.
    ///
    /// This method will panic if invoked from an asynchronous execution context. Use
    /// [`ShutdownHandle::wait`] instead.
    pub fn wait_blocking(mut self) {
        self.runtime.clone().block_on(self.wait_inner());
    }
}

/// Creates a new server.
pub(crate) fn create_server(ctx: &Arc<Context>, opts: ServerOptions) -> Arc<Server> {
    Arc::new_cyclic(|weak_self| Server::new(weak_self.clone(), ctx, opts))
}

/// A websocket server that implements the Foxglove WebSocket Protocol
pub(crate) struct Server {
    /// A weak reference to the Arc holding the server.
    /// This is used to get a reference to the outer `Arc<Server>` from Server methods.
    /// See the arc() method and its callers. We need the Arc so we can use it in async futures
    /// which need to prove to the compiler that the server will outlive the future.
    /// It's analogous to the mixin shared_from_this in C++.
    weak_self: Weak<Self>,
    context: Weak<Context>,
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
    /// Client tasks.
    tasks: parking_lot::Mutex<Option<JoinSet<()>>>,
}

impl Server {
    /// Generate a random session ID
    pub fn generate_session_id() -> String {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis().to_string())
            .unwrap_or_default()
    }

    fn new(weak_self: Weak<Self>, ctx: &Arc<Context>, opts: ServerOptions) -> Self {
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
            context: Arc::downgrade(ctx),
            message_backlog_size: opts
                .message_backlog_size
                .unwrap_or(DEFAULT_MESSAGE_BACKLOG_SIZE) as u32,
            runtime: opts.runtime.unwrap_or_else(crate::get_runtime_handle),
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
            tasks: parking_lot::Mutex::default(),
        }
    }

    fn arc(&self) -> Arc<Self> {
        self.weak_self
            .upgrade()
            .expect("server cannot be dropped while in use")
    }

    /// Returns true if the server supports the capability.
    pub(super) fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    /// Returns true if the server supports the encoding.
    pub(super) fn supports_encoding(&self, encoding: impl AsRef<str>) -> bool {
        self.supported_encodings.contains(encoding.as_ref())
    }

    /// Returns a reference to the fetch asset handler.
    pub(super) fn fetch_asset_handler(&self) -> Option<&dyn AssetHandler> {
        self.fetch_asset_handler.as_deref()
    }

    /// Returns a reference to the server listener.
    pub(super) fn listener(&self) -> Option<&dyn ServerListener> {
        self.listener.as_deref()
    }

    // Spawn a task to accept all incoming connections and return the server's local address
    pub async fn start(&self, host: &str, port: u16) -> Result<SocketAddr, FoxgloveError> {
        {
            let mut tasks = self.tasks.lock();
            if tasks.is_some() || self.cancellation_token.is_cancelled() {
                return Err(FoxgloveError::ServerAlreadyStarted);
            }
            tasks.replace(JoinSet::new());
        }

        let addr = format!("{}:{}", host, port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(FoxgloveError::Bind)?;
        let local_addr = listener.local_addr().map_err(FoxgloveError::Bind)?;

        let cancellation_token = self.cancellation_token.clone();
        let server = self.arc();
        self.runtime.spawn(async move {
            tokio::select! {
                () = server.clone().accept_connections(listener) => (),
                () = server.clone().reap_completed_tasks() => (),
                () = cancellation_token.cancelled() => (),
            }
        });

        tracing::info!("Started server on {}", local_addr);

        Ok(local_addr)
    }

    /// Accept handler which spawns a new task for each incoming connection.
    async fn accept_connections(self: Arc<Self>, listener: TcpListener) {
        while let Ok((stream, addr)) = listener.accept().await {
            if let Some(tasks) = self.tasks.lock().as_mut() {
                tasks.spawn(self.clone().handle_connection(stream, addr));
            } else {
                break;
            }
        }
    }

    /// Periodically reaps completed tasks.
    async fn reap_completed_tasks(self: Arc<Self>) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Some(tasks) = self.tasks.lock().as_mut() {
                while let Some(result) = tasks.try_join_next() {
                    process_task_result(result);
                }
            } else {
                break;
            }
        }
    }

    /// Stops the server.
    ///
    /// Returns a handle that can be used to wait for the graceful shutdown to complete. If the
    /// handle is dropped, all client tasks will be immediately aborted.
    ///
    /// Once stopped, the server cannot be restarted, and future calls to this method will return
    /// `None`.
    #[must_use]
    pub fn stop(&self) -> Option<ShutdownHandle> {
        let tasks = self.tasks.lock().take()?;
        tracing::info!("Shutting down");
        self.cancellation_token.cancel();
        let clients = self.clients.take_and_freeze();
        for client in clients.iter() {
            client.shutdown(ShutdownReason::ServerStopped);
        }
        Some(ShutdownHandle::new(self.runtime.clone(), tasks))
    }

    /// Publish the current timestamp to all clients.
    pub fn broadcast_time(&self, timestamp: u64) {
        use super::ws_protocol::server::Time;

        if !self.has_capability(Capability::Time) {
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
    pub(super) fn subscribe_parameters(&self, client_id: ClientId, names: Vec<String>) {
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
    pub(super) fn unsubscribe_parameters(&self, client_id: ClientId, names: Vec<String>) {
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
    pub(super) fn subscribe_connection_graph(&self, client_id: ClientId) -> Option<Message> {
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
    pub(super) fn unsubscribe_connection_graph(&self, client_id: ClientId) -> bool {
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
            .with_metadata(HashMap::from([(
                "fg-library".into(),
                get_library_version(),
            )]))
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
    /// - Listen for client messages
    async fn handle_connection(self: Arc<Self>, stream: TcpStream, addr: SocketAddr) {
        let Ok(mut ws_stream) = handshake::do_handshake(stream).await else {
            tracing::error!("Dropping client {addr}: handshake failed");
            return;
        };

        let message = Message::from(&self.server_info());
        if let Err(err) = ws_stream.send(message).await {
            // ServerInfo is required; do not store this client.
            tracing::error!("Failed to send required server info: {err}");
            return;
        }

        let client = ConnectedClient::new(
            &self.context,
            &self.weak_self,
            ws_stream,
            addr,
            self.message_backlog_size as usize,
        );
        self.register_client_and_advertise(&client);
        client.run().await;
        self.unregister_client(&client);
    }

    /// Registers a new client.
    fn register_client_and_advertise(&self, client: &Arc<ConnectedClient>) {
        // Add the client to self.clients. This will fail if the server is stopped.
        if !self.clients.push(client.clone()) {
            tracing::debug!("Disconnecting client {}: server is stopped", client.addr());
            client.shutdown(ShutdownReason::ServerStopped);
            return;
        }

        tracing::info!("Registered client {}", client.addr());

        // Add the client as a sink. This synchronously triggers advertisements for all channels
        // via the `Sink::add_channel` callback.
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
                    client.addr()
                );
            }
        }
    }

    /// Unregisters a client after the connection is closed.
    fn unregister_client(&self, client: &Arc<ConnectedClient>) {
        // Remove the client sink.
        if let Some(context) = self.context.upgrade() {
            context.remove_sink(client.sink_id());
        }

        self.clients.retain(|c| c.id() != client.id());
        if self.has_capability(Capability::Parameters) {
            self.unsubscribe_all_parameters(client.id());
        }
        if self.has_capability(Capability::ConnectionGraph) {
            self.unsubscribe_connection_graph(client.id());
        }
        client.on_disconnect();
        tracing::info!("Unregistered client {}", client.addr());
    }

    /// Adds new services, and advertises them to all clients.
    ///
    /// This method will fail if the services capability was not declared, or if a service name is
    /// not unique.
    pub fn add_services(&self, new_services: Vec<Service>) -> Result<(), FoxgloveError> {
        // Make sure that the server supports services.
        if !self.has_capability(Capability::Services) {
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
        for client in clients.iter() {
            for (name, id) in &new_names {
                tracing::debug!(
                    "Advertising service {name} with id {id} to client {}",
                    client.addr()
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
        for client in clients.iter() {
            for (id, name) in &old_services {
                tracing::debug!(
                    "Unadvertising service {name} with id {id} to client {}",
                    client.addr()
                );
            }
            client.send_control_msg(&msg);
        }
    }

    // Looks up a service by ID.
    pub(super) fn get_service(&self, id: ServiceId) -> Option<Arc<Service>> {
        self.services.read().get_by_id(id)
    }

    /// Sends a connection graph update to all clients.
    pub fn replace_connection_graph(
        &self,
        replacement_graph: ConnectionGraph,
    ) -> Result<(), FoxgloveError> {
        // Make sure that the server supports connection graph.
        if !self.has_capability(Capability::ConnectionGraph) {
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
