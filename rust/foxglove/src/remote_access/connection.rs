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
use tracing::{debug, error, info};

use crate::{
    Context, FoxgloveError, SinkChannelFilter,
    api_client::{DeviceToken, FoxgloveApiClientBuilder},
    library_version::get_library_version,
    protocol::v2::server::ServerInfo,
    remote_access::{
        Capability, RemoteAccessError,
        credentials_provider::CredentialsProvider,
        protocol_version,
        session::{DEFAULT_PENDING_CLIENT_READER_TIMEOUT, RemoteAccessSession, SessionParams},
    },
    remote_common::service::{Service, ServiceId, ServiceMap},
};

type Result<T> = std::result::Result<T, Box<RemoteAccessError>>;

const AUTH_RETRY_PERIOD: Duration = Duration::from_secs(30);
const DEFAULT_MESSAGE_BACKLOG_SIZE: usize = 1024;

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
pub(crate) struct ConnectionParams {
    pub name: Option<String>,
    pub device_token: String,
    pub foxglove_api_url: Option<String>,
    pub foxglove_api_timeout: Option<Duration>,
    pub listener: Option<Arc<dyn super::Listener>>,
    pub capabilities: Vec<Capability>,
    pub supported_encodings: Option<IndexSet<String>>,
    pub runtime: Handle,
    pub channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub server_info: Option<HashMap<String, String>>,
    pub message_backlog_size: Option<usize>,
    pub pending_client_reader_timeout: Option<Duration>,
    pub context: Weak<Context>,
}

/// RemoteAccessConnection manages the connected [`RemoteAccessSession`] to the LiveKit server,
/// and holds the parameters and other state that outlive a session.
pub(crate) struct RemoteAccessConnection {
    name: Option<String>,
    device_token: String,
    foxglove_api_url: Option<String>,
    foxglove_api_timeout: Option<Duration>,
    listener: Option<Arc<dyn super::Listener>>,
    capabilities: Vec<Capability>,
    supported_encodings: Option<IndexSet<String>>,
    runtime: Handle,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    server_info: Option<HashMap<String, String>>,
    message_backlog_size: Option<usize>,
    pending_client_reader_timeout: Option<Duration>,
    context: Weak<Context>,
    cancellation_token: CancellationToken,
    services: Arc<parking_lot::RwLock<ServiceMap>>,
    session: parking_lot::Mutex<Option<Arc<RemoteAccessSession>>>,
    credentials_provider: OnceCell<CredentialsProvider>,
    status: AtomicU8,
    /// The remote access session ID, received from the API server on first successful credential fetch.
    /// Set once and reused for all subsequent requests.
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
            runtime: params.runtime,
            channel_filter: params.channel_filter,
            server_info: params.server_info,
            message_backlog_size: params.message_backlog_size,
            pending_client_reader_timeout: params.pending_client_reader_timeout,
            context: params.context,
            cancellation_token: CancellationToken::new(),
            services,
            session: parking_lot::Mutex::new(None),
            credentials_provider: OnceCell::new(),
            status: AtomicU8::new(ConnectionStatus::Connecting as u8),
            remote_access_session_id: OnceLock::new(),
        }
    }

    /// Returns the current connection status.
    pub fn status(&self) -> ConnectionStatus {
        ConnectionStatus::from_u8(self.status.load(Ordering::Relaxed))
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

    /// Returns the remote access session ID, or `None` if not yet initialized.
    fn remote_access_session_id(&self) -> Option<&str> {
        self.remote_access_session_id.get().map(|s| s.as_str())
    }

    /// Returns the credentials provider, initializing it on first call.
    ///
    /// This fetches device info from the Foxglove API using the device token.
    /// If the call fails, the OnceCell remains empty and will retry on the next call.
    async fn get_or_init_provider(&self) -> Result<&CredentialsProvider> {
        self.credentials_provider
            .get_or_try_init(|| async {
                let mut builder =
                    FoxgloveApiClientBuilder::new(DeviceToken::new(self.device_token.clone()));
                if let Some(url) = &self.foxglove_api_url {
                    builder = builder.base_url(url);
                }
                if let Some(timeout) = self.foxglove_api_timeout {
                    builder = builder.timeout(timeout);
                }
                let provider = CredentialsProvider::new(builder).await?;

                let device_id = provider.device_id();
                info!(device_id, "credentials provider initialized");
                Ok(provider)
            })
            .await
    }

    async fn connect_session(
        &self,
    ) -> Result<(Arc<RemoteAccessSession>, UnboundedReceiver<RoomEvent>)> {
        let provider = self.get_or_init_provider().await?;

        let existing_session_id = self.remote_access_session_id().map(str::to_owned);
        info!(
            remote_access_session_id = existing_session_id.as_deref(),
            "requesting LiveKit credentials from API server"
        );
        let credentials = match provider
            .load_credentials(
                existing_session_id,
                Some(protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION),
            )
            .await
        {
            Ok(creds) => {
                // Set the session ID on first successful fetch.
                if let Some(ref session_id) = creds.remote_access_session_id {
                    let _ = self.remote_access_session_id.set(session_id.clone());
                }
                creds
            }
            Err(e) => {
                return Err(e.into());
            }
        };
        let remote_access_session_id = self.remote_access_session_id();
        info!(
            remote_access_session_id,
            url = credentials.url.as_str(),
            "successfully obtained LiveKit credentials"
        );

        info!(
            remote_access_session_id,
            url = credentials.url.as_str(),
            "connecting to LiveKit server"
        );
        let (session, room_events) =
            match Room::connect(&credentials.url, &credentials.token, RoomOptions::default()).await
            {
                Ok((room, room_events)) => {
                    info!(remote_access_session_id, "connected to LiveKit server");
                    let session_params = SessionParams {
                        room,
                        context: self.context.clone(),
                        channel_filter: self.channel_filter.clone(),
                        listener: self.listener.clone(),
                        capabilities: self.capabilities.clone(),
                        supported_encodings: self.supported_encodings.clone().unwrap_or_default(),
                        cancellation_token: self.cancellation_token.clone(),
                        message_backlog_size: self
                            .message_backlog_size
                            .unwrap_or(DEFAULT_MESSAGE_BACKLOG_SIZE),
                        services: self.services.clone(),
                        pending_client_reader_timeout: self
                            .pending_client_reader_timeout
                            .unwrap_or(DEFAULT_PENDING_CLIENT_READER_TIMEOUT),
                        remote_access_session_id: self
                            .remote_access_session_id()
                            .map(str::to_owned),
                    };
                    (
                        Arc::new(RemoteAccessSession::new(session_params)),
                        room_events,
                    )
                }
                Err(e) => {
                    return Err(e.into());
                }
            };

        Ok((session, room_events))
    }

    /// Run the server loop until cancelled in a new tokio task.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    pub fn spawn_run_until_cancelled(self: Arc<Self>) -> JoinHandle<()> {
        self.runtime.spawn(self.clone().run_until_cancelled())
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

    /// Connect to the room, and handle all events until cancelled or disconnected from the room.
    async fn run(&self) {
        let Some((session, room_events)) = self.connect_session_until_ok().await else {
            // Cancelled/shutting down
            debug_assert!(self.cancellation_token.is_cancelled());
            return;
        };

        self.set_status(ConnectionStatus::Connected);
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
            return;
        };
        context.add_sink(session.clone());

        // We can use spawn here because we're already running on self.runtime
        let sender_task = tokio::spawn(RemoteAccessSession::run_sender(session.clone()));

        // Send ServerInfo and channel advertisements to participants already in the room.
        // ParticipantConnected events only fire for participants joining after us.
        let server_info = self.create_server_info(remote_access_session_id.unwrap_or(""));
        for (identity, participant) in session.room().remote_participants() {
            let Some(version) = Self::check_participant_protocol_version(
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
            if let Err(e) = session
                .add_participant(identity.clone(), version, server_info.clone())
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
            _ = session.handle_room_events(room_events, server_info) => {},
            _ = session.log_periodic_stats() => {},
        }

        // Update status before cleanup so callers don't see Connected during teardown.
        if self.cancellation_token.is_cancelled() {
            self.set_status(ConnectionStatus::ShuttingDown);
        } else {
            self.set_status(ConnectionStatus::Connecting);
        }

        // Clear the active session and remove the sink before closing the room.
        *self.session.lock() = None;
        context.remove_sink(session.sink_id());
        sender_task.abort();
        // Wait for the sender task to fully stop so no callbacks are in flight.
        let _ = sender_task.await;

        info!(remote_access_session_id, "disconnecting from room");
        // Close the room (disconnect) on shutdown.
        // If we don't do that, there's a 15s delay before this device is removed from the participants
        if let Err(e) = session.room().close().await {
            error!(remote_access_session_id, error = %e, "failed to close room: {e}");
        }
    }

    /// Connect to the room, retrying indefinitely.
    ///
    /// Only returns an error if the connection has been permanently stopped/cancelled (shutting down).
    ///
    /// The retry interval is fairly long.
    /// Note that livekit internally includes a few quick retries for each connect call as well.
    async fn connect_session_until_ok(
        &self,
    ) -> Option<(Arc<RemoteAccessSession>, UnboundedReceiver<RoomEvent>)> {
        let mut interval = tokio::time::interval(AUTH_RETRY_PERIOD);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                () = self.cancellation_token.cancelled() => {
                    return None;
                }
            };

            let result = tokio::select! {
                () = self.cancellation_token.cancelled() => {
                    return None;
                }
                result = self.connect_session() => result,
            };

            let remote_access_session_id = self.remote_access_session_id();
            match result {
                Ok((session, room_events)) => {
                    return Some((session, room_events));
                }
                Err(e) => {
                    error!(
                        remote_access_session_id,
                        error = %e,
                        "connection attempt failed, will retry: {e}"
                    );
                    // Refresh credentials for auth-related errors, including room errors which
                    // may be caused by expired or invalid credentials.
                    if e.should_clear_credentials() {
                        if let Some(provider) = self.credentials_provider.get() {
                            debug!(remote_access_session_id, "clearing credentials");
                            provider.clear().await;
                        }
                    }
                }
            }
        }
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

        // The credentials provider is always initialized before this method is called,
        // since we must successfully connect (which initializes the provider) before we
        // can receive room events that trigger server info creation.
        let name = self.name.clone().unwrap_or_else(|| {
            self.credentials_provider
                .get()
                .map(|p| p.device_name().to_string())
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
    pub(crate) fn add_services(
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
    pub(crate) fn remove_services(&self, names: impl IntoIterator<Item = impl AsRef<str>>) {
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

    pub(crate) fn shutdown(&self) {
        self.cancellation_token.cancel();
    }

    /// Parse the remote access protocol version from a LiveKit participant's attributes.
    ///
    /// If the attribute is absent the participant is assumed to be running a pre-advertisement
    /// build, so we default to [`protocol_version::DEFAULT_PROTOCOL_VERSION`].
    ///
    /// Returns `None` if the attribute value is present but cannot be parsed as a semver triple.
    fn parse_participant_protocol_version(
        attributes: &HashMap<String, String>,
    ) -> Option<semver::Version> {
        let version_str = attributes
            .get("protocolVersion")
            .map(String::as_str)
            .unwrap_or(protocol_version::DEFAULT_PROTOCOL_VERSION);
        match semver::Version::parse(version_str) {
            Ok(v) => Some(v),
            Err(e) => {
                error!(
                    version = version_str,
                    "failed to parse participant protocol version: {e}"
                );
                None
            }
        }
    }

    /// Check whether a participant's protocol version meets the minimum supported version.
    ///
    /// Returns the parsed version if compatible, or `None` if the participant should be rejected.
    fn check_participant_protocol_version(
        participant_identity: &livekit::id::ParticipantIdentity,
        attributes: &HashMap<String, String>,
        remote_access_session_id: Option<&str>,
    ) -> Option<semver::Version> {
        let version = Self::parse_participant_protocol_version(attributes)?;
        let min =
            semver::Version::parse(protocol_version::REMOTE_ACCESS_MIN_SUPPORTED_PROTOCOL_VERSION)
                .expect("REMOTE_ACCESS_MIN_SUPPORTED_PROTOCOL_VERSION is a valid semver");
        if version < min {
            error!(
                remote_access_session_id,
                participant_identity = %participant_identity,
                participant_version = %version,
                min_supported_version = %min,
                "participant protocol version is below minimum supported; ignoring participant"
            );
            return None;
        }
        Some(version)
    }
}
