use std::{
    collections::HashMap,
    sync::{
        Arc, OnceLock, Weak,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
};

use indexmap::IndexSet;

use livekit::{Room, RoomEvent, RoomOptions};
use tokio::{runtime::Handle, sync::OnceCell, sync::mpsc::UnboundedReceiver, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::{
    Context, FoxgloveError, SinkChannelFilter, SinkId,
    api_client::{
        DeviceResponse, DeviceToken, FoxgloveApiClient, FoxgloveApiClientBuilder, WatchQuery,
        WatchWakeEvent,
    },
    library_version::get_library_version,
    protocol::v2::{parameter::Parameter, server::ServerInfo},
    remote_access::{
        AssetHandler, Capability, Client, RemoteAccessError,
        protocol_version::{self, REMOTE_ACCESS_PROTOCOL_VERSION},
        qos::QosClassifier,
        session::{RemoteAccessSession, SessionParams},
        watch::Watch,
        watch_loop::{
            ConnectAction, INITIAL_BACKOFF, MAX_BACKOFF, WatchAction, WatchRetryState,
            on_connect_error, on_connect_success, on_outcome,
        },
    },
    remote_common::{
        connection_graph::ConnectionGraph,
        service::{Service, ServiceId, ServiceMap},
    },
};

type Result<T> = std::result::Result<T, Box<RemoteAccessError>>;

use super::session::DEFAULT_MESSAGE_BACKLOG_SIZE;

/// The status of the remote access gateway connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConnectionStatus {
    /// The gateway is attempting to establish or re-establish a connection.
    Connecting = 0,
    /// The gateway is connected and handling events.
    Connected = 1,
    /// The gateway is shutting down. Listener callbacks may still be in progress.
    ShuttingDown = 2,
    /// The gateway has been shut down. No further listener callbacks will be invoked.
    Shutdown = 3,
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ConnectionStatus::Connecting => "connecting",
            ConnectionStatus::Connected => "connected",
            ConnectionStatus::ShuttingDown => "shutting down",
            ConnectionStatus::Shutdown => "shutdown",
        })
    }
}

impl ConnectionStatus {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Connecting,
            1 => Self::Connected,
            2 => Self::ShuttingDown,
            3 => Self::Shutdown,
            _ => unreachable!("invalid ConnectionStatus value: {value}"),
        }
    }
}

/// Parameters for constructing a [`RemoteAccessConnection`].
///
/// This should be constructed from the [`crate::remote_access::Gateway`] builder.
pub(super) struct ConnectionParams {
    pub(super) name: Option<String>,
    pub(super) device_token: String,
    pub(super) foxglove_api_url: Option<String>,
    pub(super) foxglove_api_timeout: Option<Duration>,
    pub(super) listener: Option<Arc<dyn super::Listener>>,
    pub(super) capabilities: Vec<Capability>,
    pub(super) supported_encodings: Option<IndexSet<String>>,
    pub(super) fetch_asset_handler: Option<Arc<dyn AssetHandler<Client>>>,
    pub(super) runtime: Handle,
    pub(super) channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub(super) qos_classifier: Option<Arc<dyn QosClassifier>>,
    pub(super) server_info: Option<HashMap<String, String>>,
    pub(super) message_backlog_size: Option<usize>,
    pub(super) context: Weak<Context>,
}

/// Pair of device metadata (fetched once via `fetch_device_info`) and the authenticated API
/// client used for subsequent calls (watch stream, heartbeat). Initialized lazily on the first
/// successful call to [`RemoteAccessConnection::get_or_init_device_context`].
struct DeviceContext {
    device: DeviceResponse,
    client: Arc<FoxgloveApiClient<DeviceToken>>,
}

/// A `wake` from the watch stream paired with the `device_wait_for_viewer` advertised by the
/// `hello` event of the same stream.
#[derive(Debug)]
struct WakeSignal {
    wake: WatchWakeEvent,
    device_wait_for_viewer: Duration,
}

/// RemoteAccessConnection manages the connected [`RemoteAccessSession`] to the LiveKit server,
/// and holds the parameters and other state that outlive a session.
pub(super) struct RemoteAccessConnection {
    name: Option<String>,
    device_token: String,
    foxglove_api_url: Option<String>,
    foxglove_api_timeout: Option<Duration>,
    listener: Option<Arc<dyn super::Listener>>,
    capabilities: Vec<Capability>,
    supported_encodings: Option<IndexSet<String>>,
    fetch_asset_handler: Option<Arc<dyn AssetHandler<Client>>>,
    runtime: Handle,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    qos_classifier: Option<Arc<dyn QosClassifier>>,
    server_info: Option<HashMap<String, String>>,
    message_backlog_size: Option<usize>,
    context: Weak<Context>,
    cancellation_token: CancellationToken,
    services: Arc<parking_lot::RwLock<ServiceMap>>,
    connection_graph: Arc<parking_lot::Mutex<ConnectionGraph>>,
    session: parking_lot::Mutex<Option<Arc<RemoteAccessSession>>>,
    device_context: OnceCell<DeviceContext>,
    status: AtomicU8,
    /// The remote access session ID resolved by the first `wake` event. Used purely for log
    /// correlation and for threading through subsequent watch streams as a client-provided
    /// correlation ID.
    remote_access_session_id: OnceLock<String>,
}

impl RemoteAccessConnection {
    pub fn new(params: ConnectionParams, services: Arc<parking_lot::RwLock<ServiceMap>>) -> Self {
        Self {
            name: params.name,
            device_token: params.device_token,
            foxglove_api_url: params.foxglove_api_url,
            foxglove_api_timeout: params.foxglove_api_timeout,
            listener: params.listener,
            capabilities: params.capabilities,
            supported_encodings: params.supported_encodings,
            fetch_asset_handler: params.fetch_asset_handler,
            runtime: params.runtime,
            channel_filter: params.channel_filter,
            qos_classifier: params.qos_classifier,
            server_info: params.server_info,
            message_backlog_size: params.message_backlog_size,
            context: params.context,
            cancellation_token: CancellationToken::new(),
            services,
            connection_graph: Arc::new(parking_lot::Mutex::new(ConnectionGraph::new())),
            session: parking_lot::Mutex::new(None),
            device_context: OnceCell::new(),
            status: AtomicU8::new(ConnectionStatus::Connecting as u8),
            remote_access_session_id: OnceLock::new(),
        }
    }

    /// Returns the current connection status.
    pub fn status(&self) -> ConnectionStatus {
        ConnectionStatus::from_u8(self.status.load(Ordering::Relaxed))
    }

    /// Returns the sink ID of the current session, if one is active.
    pub fn sink_id(&self) -> Option<SinkId> {
        self.session.lock().as_ref().map(|s| s.sink_id())
    }

    /// Publishes parameter values to all subscribed clients.
    ///
    /// If no session is currently active (e.g. while reconnecting), this is a no-op.
    pub fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        if let Some(session) = self.session.lock().clone() {
            session.publish_parameter_values(parameters);
        }
    }

    /// Publishes a status message to all connected participants.
    ///
    /// If no session is currently active (e.g. while reconnecting), this is a no-op.
    pub fn publish_status(&self, status: super::Status) {
        if let Some(session) = self.session.lock().clone() {
            session.publish_status(status);
        }
    }

    /// Removes status messages by ID from all connected participants.
    ///
    /// If no session is currently active (e.g. while reconnecting), this is a no-op.
    pub fn remove_status(&self, status_ids: Vec<String>) {
        if let Some(session) = self.session.lock().clone() {
            session.remove_status(status_ids);
        }
    }

    /// Replaces the connection graph and sends updates to subscribed participants.
    ///
    /// The graph state is persisted across session reconnects. If no session is currently
    /// active, the graph is still updated so that participants connecting later will
    /// receive the latest state when they subscribe.
    pub fn replace_connection_graph(
        &self,
        replacement_graph: ConnectionGraph,
    ) -> std::result::Result<(), FoxgloveError> {
        if !self.has_capability(Capability::ConnectionGraph) {
            return Err(FoxgloveError::ConnectionGraphNotSupported);
        }
        if let Some(session) = self.session.lock().clone() {
            session.replace_connection_graph(replacement_graph);
        } else {
            self.connection_graph.lock().update(replacement_graph);
        }
        Ok(())
    }

    /// Update the connection status, notifying the listener if it changed.
    fn set_status(&self, status: ConnectionStatus) {
        let prev = self.status.swap(status as u8, Ordering::Relaxed);
        if prev != status as u8
            && let Some(listener) = &self.listener
        {
            listener.on_connection_status_changed(status);
        }
    }

    /// Returns the remote access session ID resolved by the first wake event, or `None` if no wake
    /// has been received yet during this gateway's lifetime.
    fn remote_access_session_id(&self) -> Option<&str> {
        self.remote_access_session_id.get().map(String::as_str)
    }

    /// Returns the device context, initializing it on first call.
    ///
    /// This builds the authenticated API client and fetches device info. If the call fails,
    /// the OnceCell remains empty and will retry on the next call.
    async fn get_or_init_device_context(&self) -> Result<&DeviceContext> {
        self.device_context
            .get_or_try_init(|| async {
                let mut builder =
                    FoxgloveApiClientBuilder::new(DeviceToken::new(self.device_token.clone()));
                if let Some(url) = &self.foxglove_api_url {
                    builder = builder.base_url(url);
                }
                if let Some(timeout) = self.foxglove_api_timeout {
                    builder = builder.timeout(timeout);
                }
                let client = Arc::new(builder.build()?);
                let device = client.fetch_device_info().await?;
                info!(device_id = %device.id, device_name = %device.name, "device context initialized");
                Ok::<_, Box<RemoteAccessError>>(DeviceContext { device, client })
            })
            .await
    }

    /// Connects to LiveKit using the credentials delivered in a `wake` event. The
    /// `device_wait_for_viewer` is carried from the watch stream's `hello` event and applied
    /// as the session's idle timeout.
    async fn connect_session(
        &self,
        wake_signal: WakeSignal,
    ) -> Result<(Arc<RemoteAccessSession>, UnboundedReceiver<RoomEvent>)> {
        let WakeSignal {
            wake,
            device_wait_for_viewer,
        } = wake_signal;
        if let Some(session_id) = wake.remote_access_session_id.as_ref() {
            let established = self
                .remote_access_session_id
                .get_or_init(|| session_id.clone());
            if established != session_id {
                warn!(
                    remote_access_session_id = established.as_str(),
                    wake_remote_access_session_id = session_id.as_str(),
                    "wake remote access session ID differs from established ID; keeping established ID"
                );
            }
        }
        let remote_access_session_id = self.remote_access_session_id();
        info!(
            remote_access_session_id,
            url = wake.url.as_str(),
            "connecting to room"
        );
        let (room, room_events) =
            Room::connect(&wake.url, &wake.token, RoomOptions::default()).await?;
        info!(remote_access_session_id, "connected to room");
        let server_info = self.create_server_info(remote_access_session_id.unwrap_or(""));
        let session_params = SessionParams {
            room,
            context: self.context.clone(),
            channel_filter: self.channel_filter.clone(),
            qos_classifier: self.qos_classifier.clone(),
            listener: self.listener.clone(),
            capabilities: self.capabilities.clone(),
            supported_encodings: self.supported_encodings.clone().unwrap_or_default(),
            runtime: self.runtime.clone(),
            cancellation_token: self.cancellation_token.child_token(),
            message_backlog_size: self
                .message_backlog_size
                .unwrap_or(DEFAULT_MESSAGE_BACKLOG_SIZE),
            services: self.services.clone(),
            connection_graph: self.connection_graph.clone(),
            remote_access_session_id: remote_access_session_id.map(str::to_string),
            fetch_asset_handler: self.fetch_asset_handler.clone(),
            server_info,
            device_wait_for_viewer: Some(device_wait_for_viewer),
        };
        Ok((
            Arc::new(RemoteAccessSession::new(session_params)),
            room_events,
        ))
    }

    /// Run the server loop until cancelled in a new tokio task.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    pub fn spawn_run_until_cancelled(self: Arc<Self>) -> JoinHandle<()> {
        self.runtime.clone().spawn(self.run_until_cancelled())
    }

    /// Run the server loop until cancelled.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    async fn run_until_cancelled(self: Arc<Self>) {
        // Notify the listener of the initial Connecting status. The atomic is already
        // initialized to Connecting, so call the listener directly rather than going
        // through set_status (which would see no change and skip the notification).
        if let Some(listener) = &self.listener {
            listener.on_connection_status_changed(ConnectionStatus::Connecting);
        }
        while !self.cancellation_token.is_cancelled() {
            self.run().await;
        }
        // Always emit ShuttingDown before Shutdown. If run() already set ShuttingDown
        // (e.g. cancelled while connected), this is a no-op since set_status deduplicates.
        self.set_status(ConnectionStatus::ShuttingDown);
        self.set_status(ConnectionStatus::Shutdown);
    }

    /// One iteration of the outer control loop: sit dormant on the SSE watch stream until a
    /// wake arrives (or the connection is cancelled), then join LiveKit and run the session to
    /// completion. Returns when either phase has finished and it's time for the outer loop to
    /// decide whether to iterate again.
    async fn run(&self) {
        let Some(wake_signal) = self.watch_until_wake().await else {
            // Either cancelled or the device context could not be initialized; the outer loop
            // will observe cancellation or retry as appropriate.
            return;
        };
        self.run_session(wake_signal).await;
    }

    /// Runs the dormant phase. Opens the watch stream, heartbeats it, and returns on wake.
    /// Returns the wake signal with the `device_wait_for_viewer` advertised in the `hello` event
    /// of the stream on which the wake arrived. Transient failures are retried with exponential
    /// backoff (capped at [`CONTROL_LOOP_MAX_BACKOFF`]) until either a wake is received or the
    /// cancellation token fires.
    async fn watch_until_wake(&self) -> Option<WakeSignal> {
        tokio::select! {
            biased;
            () = self.cancellation_token.cancelled() => None,
            result = self.watch_until_wake_inner() => result,
        }
    }

    async fn watch_until_wake_inner(&self) -> Option<WakeSignal> {
        let device_context = self.device_context_until_ok().await?;
        let mut retry = WatchRetryState::new();
        loop {
            // Establish a watch.
            let watch = self.connect_watch(device_context, &mut retry).await?;
            let watch_lease_id = watch.lease_id().to_string();
            let device_wait_for_viewer = watch.device_wait_for_viewer();
            let heartbeat_interval = watch.heartbeat_interval();
            self.set_status(ConnectionStatus::Connected);

            // Run the watch session.
            let (outcome, watch_duration) = watch.run().await;
            match on_outcome(
                outcome,
                watch_lease_id,
                watch_duration,
                heartbeat_interval,
                &mut retry,
            ) {
                WatchAction::Wake(wake) => {
                    return Some(WakeSignal {
                        wake,
                        device_wait_for_viewer,
                    });
                }
                WatchAction::Reconnect => {
                    // Soft reconnect: try immediately and keep the user-visible status as
                    // Connected. If the next connect attempt fails, connect_watch flips to
                    // Connecting and applies its own backoff schedule.
                }
                WatchAction::Backoff { delay } => {
                    self.set_status(ConnectionStatus::Connecting);
                    tokio::time::sleep(delay).await;
                }
                WatchAction::StopUnauthorized => {
                    self.cancellation_token.cancel();
                    return None;
                }
                WatchAction::Stop => return None,
            }
        }
    }

    async fn device_context_until_ok(&self) -> Option<&DeviceContext> {
        let mut backoff = INITIAL_BACKOFF;
        let device_context = loop {
            match self.get_or_init_device_context().await {
                Ok(ctx) => break ctx,
                Err(e) => {
                    if e.is_unauthorized() {
                        error!(error = %e, "device token unauthorized; stopping remote access gateway");
                        self.cancellation_token.cancel();
                        return None;
                    }
                    warn!(error = %e, "failed to initialize device context; retrying");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        };
        Some(device_context)
    }

    async fn connect_watch(
        &self,
        device_context: &DeviceContext,
        retry: &mut WatchRetryState,
    ) -> Option<Watch> {
        loop {
            let query = WatchQuery {
                protocol_version: Some(REMOTE_ACCESS_PROTOCOL_VERSION.to_string()),
                remote_access_session_id: self.remote_access_session_id().map(str::to_string),
                // Preserve the previous lease across failed connect attempts. It is only consumed
                // once a replacement watch is established, or once the API tells us another active
                // lease owns the device.
                previous_watch_lease_id: retry.previous_lease_id().map(str::to_string),
            };
            match Watch::connect(device_context.client.clone(), query).await {
                Ok(watch) => {
                    on_connect_success(retry);
                    return Some(watch);
                }
                Err(e) => {
                    match on_connect_error(&e, retry) {
                        ConnectAction::StopUnauthorized => {
                            error!(error = %e, "device token unauthorized; stopping remote access gateway");
                            self.cancellation_token.cancel();
                            return None;
                        }
                        ConnectAction::RetryAfter(delay) => {
                            // During normal session teardown, status remains Connected. Only
                            // transition back to Connecting once the replacement watch actually
                            // fails.
                            self.set_status(ConnectionStatus::Connecting);
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }
        }
    }

    /// Joins LiveKit with the wake credentials and runs the session until cancelled,
    /// disconnected, or idle.
    async fn run_session(&self, wake_signal: WakeSignal) {
        // If connection and cancellation are both ready, prefer the completed connection so the
        // room goes through the normal session teardown path, including an explicit room close.
        let result = tokio::select! {
            biased;
            result = self.connect_session(wake_signal) => result,
            () = self.cancellation_token.cancelled() => return,
        };
        let (session, room_events) = match result {
            Ok(pair) => pair,
            Err(e) => {
                error!(error = %e, "failed to join room after wake");
                self.set_status(ConnectionStatus::Connecting);
                return;
            }
        };

        let remote_access_session_id = self.remote_access_session_id();

        // Store the active session so that external callers can reach it.
        *self.session.lock() = Some(session.clone());

        // Register the session as a sink so it receives channel notifications.
        // This synchronously triggers add_channels for all existing channels.
        let Some(context) = self.context.upgrade() else {
            info!(
                remote_access_session_id,
                "context has been dropped, stopping remote access connection"
            );
            *self.session.lock() = None;
            if let Err(e) = session.room().close().await {
                error!(remote_access_session_id, error = %e, "failed to close room: {e}");
            }
            return;
        };
        context.add_sink(session.clone());

        // We can use spawn here because we're already running on self.runtime
        let video_metadata_task = tokio::spawn(RemoteAccessSession::run_video_metadata_watcher(
            session.clone(),
        ));

        // Send ServerInfo and channel advertisements to participants already in the room.
        // ParticipantConnected events only fire for participants joining after us.
        for (identity, participant) in session.room().remote_participants() {
            let Some(version) = protocol_version::check_participant_protocol_version(
                &identity,
                &participant.attributes(),
                remote_access_session_id,
            ) else {
                // Incompatible version: send an error status and skip this participant.
                session
                    .send_incompatible_version_error(&identity, &participant.attributes())
                    .await;
                continue;
            };
            info!(
                remote_access_session_id,
                participant_identity = %identity,
                version = %version,
                "adding existing participant"
            );
            let sid = participant.sid();
            let joined_at = participant.joined_at();
            if let Err(e) = session
                .add_participant(identity.clone(), sid, joined_at, version)
                .await
            {
                error!(
                    remote_access_session_id,
                    error = %e,
                    "failed to add existing participant {identity}: {e}"
                );
            }
        }

        info!(remote_access_session_id, "running remote access server");
        tokio::select! {
            () = self.cancellation_token.cancelled() => (),
            _ = session.handle_room_events(room_events) => {},
            _ = session.log_periodic_stats() => {},
        }

        // Normal session teardown returns to the watch loop, so keep reporting Connected: the
        // gateway is still healthy and available for remote access. Cancellation is the only
        // teardown path that should move out of Connected here.
        if self.cancellation_token.is_cancelled() {
            self.set_status(ConnectionStatus::ShuttingDown);
        }

        // Clear the active session and remove the sink before closing the room.
        *self.session.lock() = None;
        {
            let mut graph = self.connection_graph.lock();
            let had_subscribers = graph.has_subscribers();
            graph.clear_subscribers();
            if had_subscribers {
                if let Some(listener) = &self.listener {
                    listener.on_connection_graph_unsubscribe();
                }
            }
        }
        context.remove_sink(session.sink_id());
        session.cancel();
        if let Err(e) = video_metadata_task.await {
            error!(
                remote_access_session_id,
                error = %e,
                "video metadata watcher failed"
            );
        }

        info!(remote_access_session_id, "disconnecting from room");
        // handle_room_events was one arm of the select! above — when the select
        // exits, that future is dropped, so no more remove_participant calls can
        // race with the flush handle drain inside close().
        session.close().await;
    }

    /// Create and serialize ServerInfo message based on the [`ConnectionParams`].
    ///
    /// The metadata and supported_encodings are important for the ClientPublish capability,
    /// as some app components will use this information to determine publish formats (ROS1 vs. JSON).
    /// For example, a ros-foxglove-bridge source may advertise the "ros1" supported encoding
    /// and "ROS_DISTRO": "melodic" metadata.
    ///
    /// We always add our own fg-library metadata.
    fn create_server_info(&self, remote_access_session_id: &str) -> ServerInfo {
        let mut metadata = self.server_info.clone().unwrap_or_default();
        let supported_encodings = self.supported_encodings.clone();
        metadata.insert("fg-library".into(), get_library_version());

        // The device context is always initialized before this method is called: it must
        // succeed before any watch stream can open, which must succeed before we can receive
        // a wake event and join LiveKit.
        let name = self.name.clone().unwrap_or_else(|| {
            self.device_context
                .get()
                .map(|ctx| ctx.device.name.clone())
                .unwrap_or_default()
        });

        let mut info = ServerInfo::new(name)
            .with_session_id(remote_access_session_id)
            .with_capabilities(
                self.capabilities
                    .iter()
                    .flat_map(|c| c.as_protocol_capabilities())
                    .copied(),
            )
            .with_metadata(metadata);

        if let Some(supported_encodings) = supported_encodings {
            info = info.with_supported_encodings(supported_encodings);
        }

        info
    }

    /// Returns true if the given capability was declared.
    fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    /// Adds new services, and advertises them to all connected participants.
    ///
    /// This method will fail if the services capability was not declared, if a service name is
    /// not unique, or if a service has no request encoding and the connection has no supported
    /// encodings.
    pub(super) fn add_services(
        &self,
        new_services: Vec<Service>,
    ) -> std::result::Result<(), FoxgloveError> {
        if !self.has_capability(Capability::Services) {
            return Err(FoxgloveError::ServicesNotSupported);
        }
        if new_services.is_empty() {
            return Ok(());
        }

        // Validate uniqueness within the batch.
        let has_supported_encodings = self
            .supported_encodings
            .as_ref()
            .is_some_and(|e| !e.is_empty());
        let mut new_names = HashMap::with_capacity(new_services.len());
        for service in &new_services {
            if new_names
                .insert(service.name().to_string(), service.id())
                .is_some()
            {
                return Err(FoxgloveError::DuplicateService(service.name().to_string()));
            }
            if service.request_encoding().is_none() && !has_supported_encodings {
                return Err(FoxgloveError::MissingRequestEncoding(
                    service.name().to_string(),
                ));
            }
        }

        // Insert into the shared service map, checking for duplicates against existing services.
        let new_service_ids: Vec<ServiceId> = {
            let mut services = self.services.write();
            for service in &new_services {
                if services.contains_name(service.name()) || services.contains_id(service.id()) {
                    return Err(FoxgloveError::DuplicateService(service.name().to_string()));
                }
            }
            let ids = new_services.iter().map(|s| s.id()).collect();
            for service in new_services {
                services.insert(service);
            }
            ids
        };

        // Notify the active session (if any) to broadcast advertisements.
        if let Some(session) = self.session.lock().as_ref() {
            session.advertise_new_services(&new_service_ids);
        }

        Ok(())
    }

    /// Removes services by name.
    ///
    /// Unrecognized service names are silently ignored.
    pub(super) fn remove_services(&self, names: impl IntoIterator<Item = impl AsRef<str>>) {
        let removed_ids: Vec<ServiceId> = {
            let mut services = self.services.write();
            names
                .into_iter()
                .filter_map(|name| services.remove_by_name(name).map(|s| s.id()))
                .collect()
        };
        if removed_ids.is_empty() {
            return;
        }
        // Notify the active session (if any) to broadcast unadvertisements.
        if let Some(session) = self.session.lock().as_ref() {
            session.unadvertise_services(&removed_ids);
        }
    }

    pub(super) fn shutdown(&self) {
        self.cancellation_token.cancel();
    }
}
