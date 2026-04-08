use crate::errors::PyFoxgloveError;
use crate::remote_common::{
    CallbackAssetHandler, PyConnectionGraph, PyParameter, PyService, PyStatusLevel,
};
use crate::{PyContext, PySinkChannelFilter};
use foxglove::websocket::{
    ChannelView, Client, ClientChannel, PlaybackCommand, PlaybackControlRequest, PlaybackState,
    PlaybackStatus, ServerListener, Status,
};
use foxglove::{WebSocketServer, WebSocketServerHandle};
use pyo3::exceptions::PyTypeError;
use pyo3::types::{PyBytes, PyTuple};
use pyo3::{prelude::*, types::PyString};
use std::sync::Arc;

/// Information about a channel.
#[pyclass(name = "ChannelView", module = "foxglove.websocket")]
pub struct PyChannelView {
    #[pyo3(get)]
    id: u64,
    #[pyo3(get)]
    topic: Py<PyString>,
}

/// Information about a client channel.
#[pyclass(name = "ClientChannel", module = "foxglove.websocket", get_all)]
pub struct PyClientChannel {
    id: u32,
    topic: Py<PyString>,
    encoding: Py<PyString>,
    schema_name: Py<PyString>,
    schema_encoding: Option<Py<PyString>>,
    schema: Option<Py<PyBytes>>,
}

/// A client connected to a running websocket server.
#[pyclass(name = "Client", module = "foxglove.websocket")]
pub struct PyClient {
    /// A client identifier that is unique within the scope of this server.
    #[pyo3(get)]
    id: u32,
}

#[pymethods]
impl PyClient {
    fn __repr__(&self) -> String {
        format!("Client(id={})", self.id)
    }
}

impl From<Client> for PyClient {
    fn from(value: Client) -> Self {
        Self {
            id: value.id().into(),
        }
    }
}

#[pyclass(name = "PlaybackStatus", module = "foxglove.websocket", eq, eq_int)]
#[derive(Clone, PartialEq)]
#[repr(u8)]
pub enum PyPlaybackStatus {
    Playing = 0,
    Paused = 1,
    Buffering = 2,
    Ended = 3,
}

impl From<PyPlaybackStatus> for PlaybackStatus {
    fn from(value: PyPlaybackStatus) -> PlaybackStatus {
        match value {
            PyPlaybackStatus::Playing => PlaybackStatus::Playing,
            PyPlaybackStatus::Paused => PlaybackStatus::Paused,
            PyPlaybackStatus::Buffering => PlaybackStatus::Buffering,
            PyPlaybackStatus::Ended => PlaybackStatus::Ended,
        }
    }
}

#[pyclass(name = "PlaybackState", module = "foxglove.websocket", eq, get_all)]
#[derive(Clone, PartialEq)]
/// The status playback of data that the server is providing
pub struct PyPlaybackState {
    /// The status of server data playback
    pub status: PyPlaybackStatus,
    /// The current time of playback, in absolute nanoseconds
    pub current_time: u64,
    /// The speed of playback, as a factor of realtime
    pub playback_speed: f32,
    /// Whether a seek forward or backward in time triggered this message to be emitted
    pub did_seek: bool,
    /// If this message is being emitted in response to a PlaybackControlRequest message, the
    /// request_id from that message. Set this to None if the state of playback has been changed
    /// by any other condition.
    pub request_id: Option<String>,
}

#[pymethods]
impl PyPlaybackState {
    #[new]
    fn new(
        status: PyPlaybackStatus,
        current_time: u64,
        playback_speed: f32,
        did_seek: bool,
        request_id: Option<String>,
    ) -> Self {
        PyPlaybackState {
            status,
            current_time,
            playback_speed,
            did_seek,
            request_id,
        }
    }
}

impl From<PyPlaybackState> for PlaybackState {
    fn from(value: PyPlaybackState) -> PlaybackState {
        PlaybackState {
            status: value.status.into(),
            current_time: value.current_time,
            playback_speed: value.playback_speed,
            did_seek: value.did_seek,
            request_id: value.request_id,
        }
    }
}

#[pyclass(name = "PlaybackCommand", module = "foxglove.websocket", eq, eq_int)]
#[derive(Clone, PartialEq)]
#[repr(u8)]
pub enum PyPlaybackCommand {
    Play = 0,
    Pause = 1,
}

impl From<PlaybackCommand> for PyPlaybackCommand {
    fn from(value: PlaybackCommand) -> PyPlaybackCommand {
        match value {
            PlaybackCommand::Play => PyPlaybackCommand::Play,
            PlaybackCommand::Pause => PyPlaybackCommand::Pause,
        }
    }
}

#[pyclass(
    name = "PlaybackControlRequest",
    module = "foxglove.websocket",
    get_all
)]
/// A request to control playback from the Foxglove app
pub struct PyPlaybackControlRequest {
    playback_command: PyPlaybackCommand,
    playback_speed: f32,
    seek_time: Option<u64>,
    request_id: String,
}
impl From<PlaybackControlRequest> for PyPlaybackControlRequest {
    fn from(value: PlaybackControlRequest) -> PyPlaybackControlRequest {
        PyPlaybackControlRequest {
            playback_command: value.playback_command.into(),
            playback_speed: value.playback_speed,
            seek_time: value.seek_time,
            request_id: value.request_id,
        }
    }
}

/// A mechanism to register callbacks for handling client message events.
///
/// Implementations of ServerListener which call the python methods. foxglove/__init__.py defines
/// the `ServerListener` protocol for callers, since a `pyclass` cannot extend Python classes:
/// https://github.com/PyO3/pyo3/issues/991
///
/// The ServerListener protocol implements all methods as no-ops by default; users extend this with
/// desired functionality.
///
/// Methods on the listener interface do not return Results; any errors are logged, assuming the
/// user has enabled logging.
pub struct PyServerListener {
    listener: Py<PyAny>,
}

impl ServerListener for PyServerListener {
    /// Callback invoked when a client subscribes to a channel.
    fn on_subscribe(&self, client: Client, channel: ChannelView) {
        let channel_id = channel.id().into();
        self.call_client_channel_method("on_subscribe", client, channel_id, channel.topic());
    }

    /// Callback invoked when a client unsubscribes from a channel.
    fn on_unsubscribe(&self, client: Client, channel: ChannelView) {
        let channel_id = channel.id().into();
        self.call_client_channel_method("on_unsubscribe", client, channel_id, channel.topic());
    }

    /// Callback invoked when a client advertises a client channel.
    fn on_client_advertise(&self, client: Client, channel: &ClientChannel) {
        let client_info = PyClient {
            id: client.id().into(),
        };

        let result: PyResult<()> = Python::with_gil(|py| {
            let py_channel = PyClientChannel {
                id: channel.id.into(),
                topic: PyString::new(py, channel.topic.as_str()).into(),
                encoding: PyString::new(py, channel.encoding.as_str()).into(),
                schema_name: PyString::new(py, channel.schema_name.as_str()).into(),
                schema_encoding: channel
                    .schema_encoding
                    .as_ref()
                    .map(|enc| PyString::new(py, enc.as_str()).into()),
                schema: channel
                    .schema
                    .as_ref()
                    .map(|schema| PyBytes::new(py, schema.as_slice()).into()),
            };

            // client, channel
            let args = (client_info, py_channel);
            self.listener
                .bind(py)
                .call_method("on_client_advertise", args, None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }

    /// Callback invoked when a client unadvertises a client channel.
    fn on_client_unadvertise(&self, client: Client, channel: &ClientChannel) {
        let client_info = PyClient {
            id: client.id().into(),
        };

        let result: PyResult<()> = Python::with_gil(|py| {
            // client, client_channel_id
            let args = (client_info, u32::from(channel.id));
            self.listener
                .bind(py)
                .call_method("on_client_unadvertise", args, None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }

    /// Callback invoked when a client message is received.
    fn on_message_data(&self, client: Client, channel: &ClientChannel, payload: &[u8]) {
        let client_info = PyClient {
            id: client.id().into(),
        };

        let result: PyResult<()> = Python::with_gil(|py| {
            // client, client_channel_id, data
            let args = (
                client_info,
                u32::from(channel.id),
                PyBytes::new(py, payload),
            );
            self.listener
                .bind(py)
                .call_method("on_message_data", args, None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }

    fn on_get_parameters(
        &self,
        client: Client,
        param_names: Vec<String>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::websocket::Parameter> {
        let client_info = PyClient {
            id: client.id().into(),
        };

        let result: PyResult<Vec<foxglove::websocket::Parameter>> = Python::with_gil(|py| {
            let args = (client_info, param_names, request_id);

            let result = self
                .listener
                .bind(py)
                .call_method("on_get_parameters", args, None)?;

            let parameters = result.extract::<Vec<PyParameter>>()?;

            Ok(parameters.into_iter().map(Into::into).collect())
        });

        match result {
            Ok(parameters) => parameters,
            Err(err) => {
                tracing::error!("Callback failed: {}", err.to_string());
                vec![]
            }
        }
    }

    fn on_set_parameters(
        &self,
        client: Client,
        parameters: Vec<foxglove::websocket::Parameter>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::websocket::Parameter> {
        let client_info = PyClient {
            id: client.id().into(),
        };

        let result: PyResult<Vec<foxglove::websocket::Parameter>> = Python::with_gil(|py| {
            let parameters: Vec<PyParameter> = parameters.into_iter().map(Into::into).collect();
            let args = (client_info, parameters, request_id);

            let result = self
                .listener
                .bind(py)
                .call_method("on_set_parameters", args, None)?;

            let parameters = result.extract::<Vec<PyParameter>>()?;

            Ok(parameters.into_iter().map(Into::into).collect())
        });

        match result {
            Ok(parameters) => parameters,
            Err(err) => {
                tracing::error!("Callback failed: {}", err.to_string());
                vec![]
            }
        }
    }

    fn on_parameters_subscribe(&self, param_names: Vec<String>) {
        let result: PyResult<()> = Python::with_gil(|py| {
            let args = (param_names,);
            self.listener
                .bind(py)
                .call_method("on_parameters_subscribe", args, None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }

    fn on_parameters_unsubscribe(&self, param_names: Vec<String>) {
        let result: PyResult<()> = Python::with_gil(|py| {
            let args = (param_names,);
            self.listener
                .bind(py)
                .call_method("on_parameters_unsubscribe", args, None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }

    fn on_connection_graph_subscribe(&self) {
        let result: PyResult<()> = Python::with_gil(|py| {
            self.listener
                .bind(py)
                .call_method("on_connection_graph_subscribe", (), None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }

    fn on_connection_graph_unsubscribe(&self) {
        let result: PyResult<()> = Python::with_gil(|py| {
            self.listener
                .bind(py)
                .call_method("on_connection_graph_unsubscribe", (), None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }

    fn on_playback_control_request(
        &self,
        playback_control_request: PlaybackControlRequest,
    ) -> Option<PlaybackState> {
        let py_playback_control_request: PyPlaybackControlRequest = playback_control_request.into();
        let result: PyResult<Option<PyPlaybackState>> = Python::with_gil(|py| {
            let result = self.listener.bind(py).call_method(
                "on_playback_control_request",
                (py_playback_control_request,),
                None,
            )?;

            result.extract::<Option<PyPlaybackState>>()
        });

        match result {
            Err(err) => {
                tracing::error!("Callback failed: {}", err.to_string());
                None
            }
            Ok(None) => None,
            Ok(Some(playback_state)) => Some(playback_state.into()),
        }
    }
}

impl PyServerListener {
    /// Call the named python method on behalf any of the ServerListener callbacks which supply a
    /// client and channel view, and return nothing.
    fn call_client_channel_method(
        &self,
        method_name: &str,
        client: Client,
        channel_id: u64,
        topic: &str,
    ) {
        let client_info = PyClient {
            id: client.id().into(),
        };

        let result: PyResult<()> = Python::with_gil(|py| {
            let channel_view = PyChannelView {
                id: channel_id,
                topic: PyString::new(py, topic).into(),
            };

            let args = (client_info, channel_view);
            self.listener
                .bind(py)
                .call_method(method_name, args, None)?;

            Ok(())
        });

        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err.to_string());
        }
    }
}

/// Start a new Foxglove WebSocket server.
#[pyfunction]
#[pyo3(signature = (*, name = None, host="127.0.0.1", port=8765, capabilities=None, server_listener=None, supported_encodings=None, services=None, asset_handler=None, context=None, session_id=None, channel_filter=None, playback_time_range = None))]
#[allow(clippy::too_many_arguments)]
pub fn start_server(
    py: Python<'_>,
    name: Option<String>,
    host: &str,
    port: u16,
    capabilities: Option<Vec<PyCapability>>,
    server_listener: Option<Py<PyAny>>,
    supported_encodings: Option<Vec<String>>,
    services: Option<Vec<PyService>>,
    asset_handler: Option<Py<PyAny>>,
    context: Option<PyRef<PyContext>>,
    session_id: Option<String>,
    channel_filter: Option<Py<PyAny>>,
    playback_time_range: Option<Py<PyTuple>>,
) -> PyResult<PyWebSocketServer> {
    let mut server = WebSocketServer::new().bind(host, port);

    if let Some(session_id) = session_id {
        server = server.session_id(session_id);
    }

    if let Some(py_obj) = server_listener {
        let listener = PyServerListener { listener: py_obj };
        server = server.listener(Arc::new(listener));
    }

    if let Some(name) = name {
        server = server.name(name);
    }

    if let Some(capabilities) = capabilities {
        server = server.capabilities(capabilities.into_iter().map(PyCapability::into));
    }

    if let Some(supported_encodings) = supported_encodings {
        server = server.supported_encodings(supported_encodings);
    }

    if let Some(services) = services {
        server = server.services(services.into_iter().map(PyService::into));
    }

    if let Some(context) = context {
        server = server.context(&context.0);
    }

    if let Some(channel_filter) = channel_filter {
        server = server.channel_filter(Arc::new(PySinkChannelFilter(channel_filter)));
    }

    if let Some(asset_handler) = asset_handler {
        server = server.fetch_asset_handler(Box::new(CallbackAssetHandler {
            handler: Arc::new(asset_handler),
        }));
    }

    if let Some(playback_time_range) = playback_time_range {
        let bound_time_range = playback_time_range.bind(py);
        if bound_time_range.len() != 2 {
            return Err(PyTypeError::new_err(
                "playback_time_range must be a tuple of (start_time, end_time)",
            ));
        }
        let start_time = bound_time_range.get_item(0)?.extract::<u64>()?;
        let end_time = bound_time_range.get_item(1)?.extract::<u64>()?;
        server = server.playback_time_range(start_time, end_time);
    }

    let handle = py
        .allow_threads(|| server.start_blocking())
        .map_err(PyFoxgloveError::from)?;

    Ok(PyWebSocketServer(Some(handle)))
}

/// A websocket server. Obtain an instance by calling :py:func:`foxglove.start_server`.
#[pyclass(name = "WebSocketServer", module = "foxglove.websocket")]
pub struct PyWebSocketServer(pub Option<WebSocketServerHandle>);

#[pymethods]
impl PyWebSocketServer {
    /// Explicitly stop the server.
    pub fn stop(&mut self, py: Python<'_>) {
        if let Some(server) = self.0.take() {
            py.allow_threads(|| server.stop().wait_blocking())
        }
    }

    /// Get the port on which the server is listening.
    #[getter]
    pub fn port(&self) -> u16 {
        self.0.as_ref().map_or(0, |handle| handle.port())
    }

    /// Returns an app URL to open the websocket as a data source.
    ///
    /// Returns None if the server has been stopped.
    ///
    /// :param layout_id: An optional layout ID to include in the URL.
    /// :type layout_id: str | None
    /// :param open_in_desktop: Opens the foxglove desktop app.
    /// :type open_in_desktop: bool
    #[pyo3(signature = (*, layout_id=None, open_in_desktop=false))]
    pub fn app_url(&self, layout_id: Option<&str>, open_in_desktop: bool) -> Option<String> {
        self.0.as_ref().map(|s| {
            let mut url = s.app_url();
            if let Some(layout_id) = layout_id {
                url = url.with_layout_id(layout_id);
            }
            if open_in_desktop {
                url = url.with_open_in_desktop();
            }
            url.to_string()
        })
    }

    /// Sets a new session ID and notifies all clients, causing them to reset their state.
    /// If no session ID is provided, generates a new one based on the current timestamp.
    /// If the server has been stopped, this has no effect.
    ///
    /// :param session_id: An optional session ID.
    /// :type session_id: str | None
    #[pyo3(signature = (session_id=None))]
    pub fn clear_session(&self, session_id: Option<String>) {
        if let Some(server) = &self.0 {
            server.clear_session(session_id);
        };
    }

    /// Publishes the current server timestamp to all clients.
    /// If the server has been stopped, this has no effect.
    ///
    /// :param timestamp_nanos: The timestamp to broadcast, in nanoseconds.
    /// :type timestamp_nanos: int
    #[pyo3(signature = (timestamp_nanos))]
    pub fn broadcast_time(&self, timestamp_nanos: u64) {
        if let Some(server) = &self.0 {
            server.broadcast_time(timestamp_nanos);
        };
    }

    /// Publish the current playback state to all clients.
    ///
    /// :param playback_state: The playback state to broadcast.
    /// :type playback_state: PlaybackState
    /// :meta private:
    #[pyo3(signature = (playback_state))]
    pub fn broadcast_playback_state(&self, playback_state: PyPlaybackState) {
        if let Some(server) = &self.0 {
            server.broadcast_playback_state(playback_state.into());
        };
    }

    /// Send a status message to all clients.
    /// If the server has been stopped, this has no effect.
    ///
    /// :param message: The message to send.
    /// :type message: str
    /// :param level: The level of the status message.
    /// :type level: StatusLevel
    /// :param id: An optional id for the status message.
    /// :type id: str | None
    #[pyo3(signature = (message, level, id=None))]
    pub fn publish_status(&self, message: String, level: &PyStatusLevel, id: Option<String>) {
        let Some(server) = &self.0 else {
            return;
        };
        let status = match id {
            Some(id) => Status::new(level.clone().into(), message).with_id(id),
            None => Status::new(level.clone().into(), message),
        };
        server.publish_status(status);
    }

    /// Remove status messages by id from all clients.
    /// If the server has been stopped, this has no effect.
    ///
    /// :param status_ids: The ids of the status messages to remove.
    /// :type status_ids: list[str]
    pub fn remove_status(&self, status_ids: Vec<String>) {
        if let Some(server) = &self.0 {
            server.remove_status(status_ids);
        };
    }

    /// Publishes parameter values to all subscribed clients.
    ///
    /// :param parameters: The parameters to publish.
    /// :type parameters: list[Parameter]
    pub fn publish_parameter_values(&self, parameters: Vec<PyParameter>) {
        if let Some(server) = &self.0 {
            server.publish_parameter_values(parameters.into_iter().map(Into::into).collect());
        }
    }

    /// Advertises support for the provided services.
    ///
    /// These services will be available for clients to use until they are removed with
    /// :py:meth:`remove_services`.
    ///
    /// This method will fail if the server was not configured with :py:attr:`Capability.Services`,
    /// if a service name is not unique, or if a service has no request encoding and the server
    /// has no supported encodings.
    ///
    /// :param services: Services to add.
    /// :type services: list[Service]
    pub fn add_services(&self, py: Python<'_>, services: Vec<PyService>) -> PyResult<()> {
        if let Some(server) = &self.0 {
            py.allow_threads(move || {
                server
                    .add_services(services.into_iter().map(|s| s.into()))
                    .map_err(PyFoxgloveError::from)
            })?;
        }
        Ok(())
    }

    /// Removes services that were previously advertised.
    ///
    /// :param names: Names of services to remove.
    /// :type names: list[str]
    pub fn remove_services(&self, py: Python<'_>, names: Vec<String>) {
        if let Some(server) = &self.0 {
            py.allow_threads(move || server.remove_services(names));
        }
    }

    /// Publishes a connection graph update to all subscribed clients. An update is published to
    /// clients as a difference from the current graph to the replacement graph. When a client first
    /// subscribes to connection graph updates, it receives the current graph.
    ///
    /// :param graph: The connection graph to publish.
    /// :type graph: ConnectionGraph
    pub fn publish_connection_graph(&self, graph: Bound<'_, PyConnectionGraph>) -> PyResult<()> {
        let Some(server) = &self.0 else {
            return Ok(());
        };
        let graph = graph.extract::<PyConnectionGraph>()?;
        server
            .publish_connection_graph(graph.into())
            .map_err(PyFoxgloveError::from)
            .map_err(PyErr::from)
    }
}

/// An enumeration of capabilities that you may choose to support for live visualization.
///
/// Specify the capabilities you support when calling :py:func:`start_server`. These will be
/// advertised to the Foxglove app when connected as a WebSocket client.
#[pyclass(name = "Capability", module = "foxglove.websocket", eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyCapability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
    /// Allow clients to subscribe and make connection graph updates
    ConnectionGraph,
    /// Allow clients to get & set parameters.
    Parameters,
    /// Inform clients about the latest server time.
    ///
    /// This allows accelerated, slowed, or stepped control over the progress of time. If the
    /// server publishes time data, then timestamps of published messages must originate from the
    /// same time source.
    Time,
    /// Allow clients to call services.
    Services,
    /// Indicates that the server is capable of responding to playback control requests from
    /// controls in the Foxglove app. This requires the server to specify the `data_start_time`
    /// and `data_end_time` fields in its `ServerInfo` message.
    PlaybackControl,
}

impl From<PyCapability> for foxglove::websocket::Capability {
    fn from(value: PyCapability) -> Self {
        match value {
            PyCapability::ClientPublish => foxglove::websocket::Capability::ClientPublish,
            PyCapability::ConnectionGraph => foxglove::websocket::Capability::ConnectionGraph,
            PyCapability::Parameters => foxglove::websocket::Capability::Parameters,
            PyCapability::Time => foxglove::websocket::Capability::Time,
            PyCapability::Services => foxglove::websocket::Capability::Services,
            PyCapability::PlaybackControl => foxglove::websocket::Capability::PlaybackControl,
        }
    }
}

/// Container for a user-defined asset handler.
///
/// The handler must be a callback function which takes the uri as its argument, and returns the
/// asset `bytes`. If the handler returns `None`, a "not found" message will be sent to the client.
/// If the handler raises an exception, the stringified exception message will be returned to the
/// client as an error.
pub fn register_submodule(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let module = PyModule::new(parent_module.py(), "websocket")?;

    module.add_class::<PyWebSocketServer>()?;
    module.add_class::<PyCapability>()?;
    module.add_class::<PyClient>()?;
    module.add_class::<PyClientChannel>()?;
    module.add_class::<PyChannelView>()?;
    module.add_class::<PyPlaybackCommand>()?;
    module.add_class::<PyPlaybackControlRequest>()?;
    module.add_class::<PyPlaybackStatus>()?;
    module.add_class::<PyPlaybackState>()?;
    // Define as a package
    // https://github.com/PyO3/pyo3/issues/759
    let py = parent_module.py();
    py.import("sys")?
        .getattr("modules")?
        .set_item("foxglove._foxglove_py.websocket", &module)?;

    parent_module.add_submodule(&module)
}
