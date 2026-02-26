use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use indexmap::IndexSet;

use livekit::{Room, RoomEvent, RoomOptions};
use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver, sync::OnceCell, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    api_client::{DeviceToken, FoxgloveApiClientBuilder},
    library_version::get_library_version,
    protocol::v2::server::ServerInfo,
    remote_access::{
        credentials_provider::CredentialsProvider, session::RemoteAccessSession, Capability,
        RemoteAccessError,
    },
    Context, SinkChannelFilter,
};

type Result<T> = std::result::Result<T, RemoteAccessError>;

const AUTH_RETRY_PERIOD: Duration = Duration::from_secs(30);
const DEFAULT_MESSAGE_BACKLOG_SIZE: usize = 1024;

/// Generate a session ID from the current timestamp.
fn generate_session_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis().to_string())
        .unwrap_or_default()
}

/// Options for the remote access connection.
///
/// This should be constructed from the [`crate::remote_access::Gateway`] builder.
#[derive(Clone)]
pub(crate) struct RemoteAccessConnectionOptions {
    pub name: Option<String>,
    pub device_token: String,
    pub foxglove_api_url: Option<String>,
    pub foxglove_api_timeout: Option<Duration>,
    pub session_id: String,
    pub listener: Option<Arc<dyn super::Listener>>,
    pub capabilities: Vec<Capability>,
    pub supported_encodings: Option<IndexSet<String>>,
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
            session_id: generate_session_id(),
            listener: None,
            capabilities: Vec::new(),
            supported_encodings: None,
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
            .field("session_id", &self.session_id)
            .field("has_listener", &self.listener.is_some())
            .field("capabilities", &self.capabilities)
            .field("supported_encodings", &self.supported_encodings)
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
    credentials_provider: OnceCell<CredentialsProvider>,
}

impl RemoteAccessConnection {
    pub fn new(options: RemoteAccessConnectionOptions) -> Self {
        Self {
            options,
            credentials_provider: OnceCell::new(),
        }
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
                Ok(CredentialsProvider::new(builder).await?)
            })
            .await
    }

    async fn connect_session(
        &self,
    ) -> Result<(Arc<RemoteAccessSession>, UnboundedReceiver<RoomEvent>)> {
        let provider = self.get_or_init_provider().await?;
        let credentials = provider.load_credentials().await?;

        let message_backlog_size = self
            .options
            .message_backlog_size
            .unwrap_or(DEFAULT_MESSAGE_BACKLOG_SIZE);

        let (session, room_events) =
            match Room::connect(&credentials.url, &credentials.token, RoomOptions::default()).await
            {
                Ok((room, room_events)) => (
                    Arc::new(RemoteAccessSession::new(
                        room,
                        self.options.context.clone(),
                        self.options.channel_filter.clone(),
                        self.options.cancellation_token.clone(),
                        message_backlog_size,
                    )),
                    room_events,
                ),
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
        while !self.cancellation_token().is_cancelled() {
            self.run().await;
        }
    }

    /// Connect to the room, and handle all events until cancelled or disconnected from the room.
    async fn run(&self) {
        let Some((session, room_events)) = self.connect_session_until_ok().await else {
            // Cancelled/shutting down
            debug_assert!(self.cancellation_token().is_cancelled());
            return;
        };

        // Register the session as a sink so it receives channel notifications.
        // This synchronously triggers add_channels for all existing channels.
        let Some(context) = self.options.context.upgrade() else {
            info!("context has been dropped, stopping remote access connection");
            return;
        };
        context.add_sink(session.clone());

        // We can use spawn here because we're already running on self.options.runtime (if set)
        let sender_task = tokio::spawn(RemoteAccessSession::run_sender(session.clone()));

        let attributes = session.room().local_participant().attributes();
        let identity = session.room().local_participant().identity();
        info!(
            "local participant {:?} attributes: {:?}",
            identity, attributes
        );

        info!("running remote access server");
        tokio::select! {
            () = self.cancellation_token().cancelled() => (),
            _ = self.listen_for_room_events(session.clone(), room_events) => {}
        }

        // Remove the sink before closing the room.
        context.remove_sink(session.sink_id());
        sender_task.abort();

        info!("disconnecting from room");
        // Close the room (disconnect) on shutdown.
        // If we don't do that, there's a 15s delay before this device is removed from the participants
        if let Err(e) = session.room().close().await {
            error!("failed to close room: {e:?}");
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

            match result {
                Ok((session, room_events)) => {
                    return Some((session, room_events));
                }
                Err(e) => {
                    error!("{e}");
                    // Refresh credentials for auth-related errors, including room errors which
                    // may be caused by expired or invalid credentials.
                    if e.should_clear_credentials() {
                        if let Some(provider) = self.credentials_provider.get() {
                            debug!("clearing credentials");
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
        while let Some(event) = room_events.recv().await {
            debug!("room event: {:?}", event);
            match event {
                RoomEvent::ParticipantConnected(participant) => {
                    info!("entered the room: {:?}", participant.identity());
                    let participant_id = match session.add_participant(participant.identity()).await
                    {
                        Ok(id) => id,
                        Err(e) => {
                            error!("failed to add participant: {e:?}");
                            continue;
                        }
                    };

                    let server_info = self.create_server_info();
                    session.send_info_and_advertisements(participant_id.clone(), server_info);
                }
                RoomEvent::ParticipantDisconnected(participant) => {
                    session.remove_participant(&participant.identity());
                }
                RoomEvent::DataReceived {
                    payload: _,
                    topic,
                    kind: _,
                    participant: _,
                } => {
                    info!("data received: {:?}", topic);
                }
                RoomEvent::ByteStreamOpened {
                    reader,
                    topic: _,
                    participant_identity,
                } => {
                    // This is how we handle incoming reliable messages from the client
                    // They open a byte stream to the device participant (us).
                    info!(
                        "byte stream opened from participant: {:?}",
                        participant_identity
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
                RoomEvent::Disconnected { reason } => {
                    info!(
                        "disconnected: {:?}, will attempt to reconnect",
                        reason.as_str_name()
                    );
                    // Return from this function to trigger reconnection in run_until_cancelled
                    return;
                }
                _ => {}
            }
        }
        warn!("stopped listening for room events");
    }

    /// Create and serialize ServerInfo message based on the RemoteAccessConnectionOptions.
    ///
    /// The metadata and supported_encodings are important for the ClientPublish capability,
    /// as some app components will use this information to determine publish formats (ROS1 vs. JSON).
    /// For example, a ros-foxglove-bridge source may advertise the "ros1" supported encoding
    /// and "ROS_DISTRO": "melodic" metadata.
    ///
    /// We always add our own fg-library metadata.
    pub fn create_server_info(&self) -> ServerInfo {
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
            .with_session_id(self.options.session_id.clone())
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
