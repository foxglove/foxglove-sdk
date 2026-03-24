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
use tracing::{debug, error, info, warn};

use crate::{
    Context, SinkChannelFilter,
    api_client::{DeviceToken, FoxgloveApiClientBuilder},
    library_version::get_library_version,
    protocol::v2::server::ServerInfo,
    remote_access::{
        Capability, RemoteAccessError, credentials_provider::CredentialsProvider,
        session::RemoteAccessSession,
    },
    remote_common::service::{Service, ServiceMap},
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

/// Options for the remote access connection.
///
/// This should be constructed from the [`crate::remote_access::Gateway`] builder.
pub(crate) struct RemoteAccessConnectionOptions {
    pub name: Option<String>,
    pub device_token: String,
    pub foxglove_api_url: Option<String>,
    pub foxglove_api_timeout: Option<Duration>,
    pub listener: Option<Arc<dyn super::Listener>>,
    pub capabilities: Vec<Capability>,
    pub supported_encodings: Option<IndexSet<String>>,
    pub services: HashMap<String, Service>,
    pub runtime: Option<Handle>,
    pub channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub server_info: Option<HashMap<String, String>>,
    pub message_backlog_size: Option<usize>,
    pub cancellation_token: CancellationToken,
    pub context: Weak<Context>,
}

impl Default for RemoteAccessConnectionOptions {
    fn default() -> Self {
        Self {
            name: None,
            device_token: String::new(),
            foxglove_api_url: None,
            foxglove_api_timeout: None,
            listener: None,
            capabilities: Vec::new(),
            supported_encodings: None,
            services: HashMap::new(),
            runtime: None,
            channel_filter: None,
            server_info: None,
            message_backlog_size: None,
            cancellation_token: CancellationToken::new(),
            context: Arc::downgrade(&Context::get_default()),
        }
    }
}

impl std::fmt::Debug for RemoteAccessConnectionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAccessConnectionOptions")
            .field("name", &self.name)
            .field("has_device_token", &!self.device_token.is_empty())
            .field("foxglove_api_url", &self.foxglove_api_url)
            .field("foxglove_api_timeout", &self.foxglove_api_timeout)
            .field("has_listener", &self.listener.is_some())
            .field("capabilities", &self.capabilities)
            .field("supported_encodings", &self.supported_encodings)
            .field("num_services", &self.services.len())
            .field("has_runtime", &self.runtime.is_some())
            .field("has_channel_filter", &self.channel_filter.is_some())
            .field("server_info", &self.server_info)
            .field("message_backlog_size", &self.message_backlog_size)
            .field("has_context", &(self.context.strong_count() > 0))
            .finish()
    }
}

/// RemoteAccessConnection manages the connected [`RemoteAccessSession`] to the LiveKit server,
/// and holds the options and other state that outlive a session.
pub(crate) struct RemoteAccessConnection {
    options: RemoteAccessConnectionOptions,
    services: Arc<ServiceMap>,
    credentials_provider: OnceCell<CredentialsProvider>,
    status: AtomicU8,
    /// The remote access session ID, received from the API server on first successful credential fetch.
    /// Set once and reused for all subsequent requests.
    remote_access_session_id: OnceLock<String>,
}

impl RemoteAccessConnection {
    pub fn new(mut options: RemoteAccessConnectionOptions) -> Self {
        let services = Arc::new(ServiceMap::from_iter(
            options.services.drain().map(|(_, s)| s),
        ));
        Self {
            options,
            services,
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
            && let Some(listener) = &self.options.listener
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
                let mut builder = FoxgloveApiClientBuilder::new(DeviceToken::new(
                    self.options.device_token.clone(),
                ));
                if let Some(url) = &self.options.foxglove_api_url {
                    builder = builder.base_url(url);
                }
                if let Some(timeout) = self.options.foxglove_api_timeout {
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
        let credentials = match provider.load_credentials(existing_session_id).await {
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

        let message_backlog_size = self
            .options
            .message_backlog_size
            .unwrap_or(DEFAULT_MESSAGE_BACKLOG_SIZE);

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
                    (
                        Arc::new(RemoteAccessSession::new(
                            &self.options,
                            room,
                            message_backlog_size,
                            self.services.clone(),
                        )),
                        room_events,
                    )
                }
                Err(e) => {
                    return Err(e.into());
                }
            };

        Ok((session, room_events))
    }

    /// Returns the cancellation token for the [`RemoteAccessConnection`]`.
    fn cancellation_token(&self) -> &CancellationToken {
        &self.options.cancellation_token
    }

    /// Run the server loop until cancelled in a new tokio task.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    pub fn spawn_run_until_cancelled(self: Arc<Self>) -> JoinHandle<()> {
        if let Some(runtime) = self.options.runtime.as_ref() {
            runtime.spawn(self.clone().run_until_cancelled())
        } else {
            tokio::spawn(self.run_until_cancelled())
        }
    }

    /// Run the server loop until cancelled.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    async fn run_until_cancelled(self: Arc<Self>) {
        // Notify the listener of the initial Connecting status. The atomic is already
        // initialized to Connecting, so call the listener directly rather than going
        // through set_status (which would see no change and skip the notification).
        if let Some(listener) = &self.options.listener {
            listener.on_connection_status_changed(ConnectionStatus::Connecting);
        }
        while !self.cancellation_token().is_cancelled() {
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
            debug_assert!(self.cancellation_token().is_cancelled());
            return;
        };

        self.set_status(ConnectionStatus::Connected);
        let remote_access_session_id = self.remote_access_session_id();

        // Register the session as a sink so it receives channel notifications.
        // This synchronously triggers add_channels for all existing channels.
        let Some(context) = self.options.context.upgrade() else {
            info!(
                remote_access_session_id,
                "context has been dropped, stopping remote access connection"
            );
            return;
        };
        context.add_sink(session.clone());

        // We can use spawn here because we're already running on self.options.runtime (if set)
        let sender_task = tokio::spawn(RemoteAccessSession::run_sender(session.clone()));

        // Send ServerInfo and channel advertisements to participants already in the room.
        // ParticipantConnected events only fire for participants joining after us.
        let server_info = self.create_server_info(remote_access_session_id.unwrap_or(""));
        for (identity, _) in session.room().remote_participants() {
            if let Err(e) = session
                .add_participant(identity.clone(), server_info.clone())
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
            () = self.cancellation_token().cancelled() => (),
            _ = self.listen_for_room_events(session.clone(), room_events) => {},
            _ = Self::log_periodic_stats(session.clone(), remote_access_session_id.unwrap_or("").to_owned()) => {},
        }

        // Update status before cleanup so callers don't see Connected during teardown.
        if self.cancellation_token().is_cancelled() {
            self.set_status(ConnectionStatus::ShuttingDown);
        } else {
            self.set_status(ConnectionStatus::Connecting);
        }

        // Remove the sink before closing the room.
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
                () = self.cancellation_token().cancelled() => {
                    return None;
                }
            };

            let result = tokio::select! {
                () = self.cancellation_token().cancelled() => {
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

    async fn listen_for_room_events(
        &self,
        session: Arc<RemoteAccessSession>,
        mut room_events: UnboundedReceiver<RoomEvent>,
    ) {
        let remote_access_session_id = self.remote_access_session_id();
        while let Some(event) = room_events.recv().await {
            match event {
                RoomEvent::ParticipantConnected(participant) => {
                    let participant_identity = participant.identity();
                    info!(
                        remote_access_session_id,
                        participant_identity = %participant_identity,
                        "participant connected to room"
                    );
                    let server_info =
                        self.create_server_info(remote_access_session_id.unwrap_or(""));
                    if let Err(e) = session
                        .add_participant(participant.identity(), server_info)
                        .await
                    {
                        error!(remote_access_session_id, error = %e, "failed to add participant: {e}");
                        continue;
                    }
                }
                RoomEvent::ParticipantDisconnected(participant) => {
                    info!(
                        remote_access_session_id,
                        participant_identity = %participant.identity(),
                        "participant disconnected from room"
                    );
                    session.remove_participant(&participant.identity());
                }
                RoomEvent::DataReceived {
                    payload: _,
                    topic,
                    kind: _,
                    participant: _,
                } => {
                    info!(remote_access_session_id, "data received: {:?}", topic);
                }
                RoomEvent::ByteStreamOpened {
                    reader,
                    topic: _,
                    participant_identity,
                } => {
                    // This is how we handle incoming reliable messages from the client
                    // They open a byte stream to the device participant (us).
                    info!(
                        remote_access_session_id,
                        participant_identity = %participant_identity,
                        "byte stream opened from participant"
                    );
                    if let Some(reader) = reader.take() {
                        let session2 = session.clone();
                        tokio::spawn(async move {
                            session2
                                .handle_byte_stream_from_client(participant_identity, reader)
                                .await;
                        });
                    }
                }
                RoomEvent::ConnectionStateChanged(state) => {
                    info!(
                        remote_access_session_id,
                        state = ?state,
                        "connection state changed"
                    );
                }
                RoomEvent::Reconnecting => {
                    info!(remote_access_session_id, "reconnecting to room");
                }
                RoomEvent::Reconnected => {
                    info!(remote_access_session_id, "reconnected to room");
                }
                RoomEvent::ConnectionQualityChanged {
                    quality,
                    participant,
                } => {
                    info!(
                        remote_access_session_id,
                        participant = %participant.identity(),
                        quality = ?quality,
                        "connection quality changed"
                    );
                }
                RoomEvent::TrackSubscriptionFailed {
                    participant,
                    error,
                    track_sid,
                } => {
                    warn!(
                        remote_access_session_id,
                        participant = %participant.identity(),
                        track_sid = %track_sid,
                        error = %error,
                        "track subscription failed: {error}"
                    );
                }
                RoomEvent::LocalTrackPublished {
                    publication,
                    track: _,
                    participant: _,
                } => {
                    info!(
                        remote_access_session_id,
                        track_sid = %publication.sid(),
                        track_name = %publication.name(),
                        "local track published"
                    );
                }
                RoomEvent::LocalTrackUnpublished {
                    publication,
                    participant: _,
                } => {
                    info!(
                        remote_access_session_id,
                        track_sid = %publication.sid(),
                        track_name = %publication.name(),
                        "local track unpublished"
                    );
                }
                RoomEvent::TrackSubscribed {
                    track: _,
                    publication,
                    participant,
                } => {
                    info!(
                        remote_access_session_id,
                        participant = %participant.identity(),
                        track_sid = %publication.sid(),
                        track_name = %publication.name(),
                        "remote track subscribed"
                    );
                }
                RoomEvent::TrackUnsubscribed {
                    track: _,
                    publication,
                    participant,
                } => {
                    info!(
                        remote_access_session_id,
                        participant = %participant.identity(),
                        track_sid = %publication.sid(),
                        track_name = %publication.name(),
                        "remote track unsubscribed"
                    );
                }
                RoomEvent::TrackMuted {
                    participant,
                    publication,
                } => {
                    info!(
                        remote_access_session_id,
                        participant = %participant.identity(),
                        track_sid = %publication.sid(),
                        track_name = %publication.name(),
                        "track muted"
                    );
                }
                RoomEvent::TrackUnmuted {
                    participant,
                    publication,
                } => {
                    info!(
                        remote_access_session_id,
                        participant = %participant.identity(),
                        track_sid = %publication.sid(),
                        track_name = %publication.name(),
                        "track unmuted"
                    );
                }
                RoomEvent::Disconnected { reason } => {
                    info!(
                        remote_access_session_id,
                        reason = reason.as_str_name(),
                        "disconnected from room, will attempt to reconnect"
                    );
                    // Return from this function to trigger reconnection in run_until_cancelled
                    return;
                }
                _ => {
                    debug!(remote_access_session_id, "room event: {:?}", event);
                }
            }
        }
        warn!(
            remote_access_session_id,
            "stopped listening for room events"
        );
    }

    /// Periodically logs session statistics for monitoring and debugging.
    async fn log_periodic_stats(
        session: Arc<RemoteAccessSession>,
        remote_access_session_id: String,
    ) {
        let period = Duration::from_secs(30);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let stats = session.stats();
            let connection_quality = session.room().local_participant().connection_quality();
            let total_video_bytes_sent = match session.room().get_stats().await {
                Ok(stats) => Some(
                    stats
                        .publisher_stats
                        .iter()
                        .filter_map(|s| match s {
                            libwebrtc::stats::RtcStats::OutboundRtp(rtp)
                                if rtp.stream.kind == "video" =>
                            {
                                Some(rtp.sent.bytes_sent)
                            }
                            _ => None,
                        })
                        .sum::<u64>(),
                ),
                Err(e) => {
                    warn!(remote_access_session_id, error = %e, "failed to get room stats: {e}");
                    None
                }
            };
            info!(
                remote_access_session_id,
                participants = stats.participants,
                subscriptions = stats.subscriptions,
                video_tracks = stats.video_tracks,
                total_video_bytes_sent,
                connection_quality = ?connection_quality,
                "periodic stats"
            );
        }
    }

    /// Create and serialize ServerInfo message based on the RemoteAccessConnectionOptions.
    ///
    /// The metadata and supported_encodings are important for the ClientPublish capability,
    /// as some app components will use this information to determine publish formats (ROS1 vs. JSON).
    /// For example, a ros-foxglove-bridge source may advertise the "ros1" supported encoding
    /// and "ROS_DISTRO": "melodic" metadata.
    ///
    /// We always add our own fg-library metadata.
    fn create_server_info(&self, remote_access_session_id: &str) -> ServerInfo {
        let mut metadata = self.options.server_info.clone().unwrap_or_default();
        let supported_encodings = self.options.supported_encodings.clone();
        metadata.insert("fg-library".into(), get_library_version());

        // The credentials provider is always initialized before this method is called,
        // since we must successfully connect (which initializes the provider) before we
        // can receive room events that trigger server info creation.
        let name = self.options.name.clone().unwrap_or_else(|| {
            self.credentials_provider
                .get()
                .map(|p| p.device_name().to_string())
                .unwrap_or_default()
        });

        let mut info = ServerInfo::new(name)
            .with_session_id(remote_access_session_id)
            .with_capabilities(
                self.options
                    .capabilities
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

    pub(crate) fn shutdown(&self) {
        self.cancellation_token().cancel();
    }
}
