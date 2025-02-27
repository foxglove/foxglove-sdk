use crate::errors::PyFoxgloveError;
use bytes::Bytes;
use foxglove::{
    websocket::{ChannelView, Client, ClientChannelView, ServerListener, Status, StatusLevel},
    WebSocketServer, WebSocketServerBlockingHandle,
};
use pyo3::{
    prelude::*,
    types::{PyBytes, PyString},
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time;

/// Information about a channel.
#[pyclass(name = "ChannelView", module = "foxglove")]
pub struct PyChannelView {
    #[pyo3(get)]
    id: u64,
    #[pyo3(get)]
    topic: Py<PyString>,
}

/// A client connected to a running websocket server.
#[pyclass(name = "Client", module = "foxglove")]
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
    fn on_client_advertise(&self, client: Client, channel: ClientChannelView) {
        let channel_id = channel.id().into();
        self.call_client_channel_method("on_client_advertise", client, channel_id, channel.topic());
    }

    /// Callback invoked when a client unadvertises a client channel.
    fn on_client_unadvertise(&self, client: Client, channel: ClientChannelView) {
        let channel_id = channel.id().into();
        self.call_client_channel_method(
            "on_client_unadvertise",
            client,
            channel_id,
            channel.topic(),
        );
    }

    /// Callback invoked when a client message is received.
    fn on_message_data(&self, client: Client, channel: ClientChannelView, payload: &[u8]) {
        let client_info = PyClient {
            id: client.id().into(),
        };

        let result: PyResult<()> = Python::with_gil(|py| {
            let channel_view = PyChannelView {
                id: channel.id().into(),
                topic: PyString::new(py, channel.topic()).into(),
            };

            // client, channel, data
            let args = (client_info, channel_view, PyBytes::new(py, payload));
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

/// A handler for websocket services which calls out to user-defined functions
struct ServiceHandler {
    handler: Arc<Py<PyAny>>,
}
impl foxglove::websocket::service::Handler for ServiceHandler {
    fn call(
        &self,
        request: foxglove::websocket::service::Request,
        responder: foxglove::websocket::service::Responder,
    ) {
        let handler = self.handler.clone();
        let request = PyRequest(request);
        // Punt the callback to a blocking thread.
        tokio::task::spawn_blocking(move || {
            let result = Python::with_gil(|py| {
                handler
                    .bind(py)
                    .call((request,), None)
                    .and_then(|data| data.extract::<Vec<u8>>())
            })
            .map(Bytes::from)
            .map_err(|e| e.to_string());
            responder.respond(result);
        });
    }
}

/// Start a new Foxglove WebSocket server.
///
/// :param name: The name of the server.
/// :param host: The host to bind to.
/// :param port: The port to bind to.
/// :param capabilities: A list of capabilities to advertise to clients.
/// :param server_listener: A Python object that implements the :py:class:`ServerListener` protocol.
/// :param supported_encodings: A list of encodings to advertise to clients.
///    Foxglove currently supports "json", "ros1", and "cdr" for client-side publishing.
///
/// To connect to this server: open Foxglove, choose "Open a new connection", and select Foxglove
/// WebSocket. The default connection string matches the defaults used by the SDK.
#[pyfunction]
#[pyo3(signature = (*, name = None, host="127.0.0.1", port=8765, capabilities=None, server_listener=None, supported_encodings=None, services=None))]
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
) -> PyResult<PyWebSocketServer> {
    let session_id = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .expect("Failed to create session ID; invalid system time")
        .as_millis()
        .to_string();

    let mut server = WebSocketServer::new()
        .session_id(session_id)
        .bind(host, port);

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

    let handle = py
        .allow_threads(|| server.start_blocking())
        .map_err(PyFoxgloveError::from)?;

    Ok(PyWebSocketServer(Some(handle)))
}

/// A live visualization server. Obtain an instance by calling :py:func:`start_server`.
#[pyclass(name = "WebSocketServer", module = "foxglove")]
pub struct PyWebSocketServer(pub Option<WebSocketServerBlockingHandle>);

#[pymethods]
impl PyWebSocketServer {
    pub fn stop(&mut self, py: Python<'_>) {
        if let Some(server) = self.0.take() {
            py.allow_threads(|| server.stop())
        }
    }

    /// Sets a new session ID and notifies all clients, causing them to reset their state.
    /// If no session ID is provided, generates a new one based on the current timestamp.
    /// If the server has been stopped, this has no effect.
    #[pyo3(signature = (session_id=None))]
    pub fn clear_session(&self, session_id: Option<String>) {
        if let Some(server) = &self.0 {
            server.clear_session(session_id);
        };
    }

    /// Publishes the current server timestamp to all clients.
    /// If the server has been stopped, this has no effect.
    pub fn broadcast_time(&self, timestamp_nanos: u64) {
        if let Some(server) = &self.0 {
            server.broadcast_time(timestamp_nanos);
        };
    }

    /// Send a status message to all clients.
    /// If the server has been stopped, this has no effect.
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
    pub fn remove_status(&self, status_ids: Vec<String>) {
        if let Some(server) = &self.0 {
            server.remove_status(status_ids);
        };
    }

    /// Publishes parameter values to all subscribed clients.
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
    /// This method will fail if the server was not configured with :py:attr:`Capability.Services`.
    ///
    /// :param services: Services to add.
    /// :type services: list[:py:class:`Service`]
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
}

/// The level of a :py:class:`Status` message
#[pyclass(name = "StatusLevel", module = "foxglove", eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyStatusLevel {
    Info,
    Warning,
    Error,
}

impl From<PyStatusLevel> for StatusLevel {
    fn from(value: PyStatusLevel) -> Self {
        match value {
            PyStatusLevel::Info => StatusLevel::Info,
            PyStatusLevel::Warning => StatusLevel::Warning,
            PyStatusLevel::Error => StatusLevel::Error,
        }
    }
}

/// A capability that the websocket server advertises to its clients.
#[pyclass(name = "Capability", module = "foxglove", eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyCapability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
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
}

impl From<PyCapability> for foxglove::websocket::Capability {
    fn from(value: PyCapability) -> Self {
        match value {
            PyCapability::ClientPublish => foxglove::websocket::Capability::ClientPublish,
            PyCapability::Parameters => foxglove::websocket::Capability::Parameters,
            PyCapability::Time => foxglove::websocket::Capability::Time,
            PyCapability::Services => foxglove::websocket::Capability::Services,
        }
    }
}

/// A websocket service.
///
/// The handler must be a callback function which takes the :py:class:`Client` and
/// :py:class:`Request` as arguments, and returns `bytes` as a response. If the handler raises an
/// exception, the stringified exception message will be returned to the client as an error.
#[pyclass(name = "Service", module = "foxglove", get_all, set_all)]
#[derive(FromPyObject)]
pub struct PyService {
    name: String,
    schema: PyServiceSchema,
    handler: Py<PyAny>,
}

#[pymethods]
impl PyService {
    /// Create a new service.
    #[new]
    #[pyo3(signature = (name, *, schema, handler))]
    fn new(name: &str, schema: PyServiceSchema, handler: Py<PyAny>) -> Self {
        PyService {
            name: name.to_string(),
            schema,
            handler,
        }
    }
}

impl From<PyService> for foxglove::websocket::service::Service {
    fn from(value: PyService) -> Self {
        foxglove::websocket::service::Service::builder(value.name, value.schema.into()).handler(
            ServiceHandler {
                handler: Arc::new(value.handler),
            },
        )
    }
}

/// A websocket service request.
#[pyclass(name = "Request", module = "foxglove")]
pub struct PyRequest(foxglove::websocket::service::Request);

#[pymethods]
impl PyRequest {
    /// The service name.
    #[getter]
    fn service_name(&self) -> &str {
        self.0.service_name()
    }

    /// The client ID.
    #[getter]
    fn client_id(&self) -> u32 {
        self.0.client_id().into()
    }

    /// The call ID that uniquely identifies this request for this client.
    #[getter]
    fn call_id(&self) -> u32 {
        self.0.call_id().into()
    }

    /// The request encoding.
    #[getter]
    fn encoding(&self) -> &str {
        self.0.encoding()
    }

    /// The request payload.
    #[getter]
    fn payload(&self) -> &[u8] {
        self.0.payload()
    }
}

/// A service schema.
///
/// :param name: The name of the service.
/// :type name: str
/// :param request: The request schema.
/// :type request: :py:class:`MessageSchema` | `None`
/// :param response: The response schema.
/// :type response: :py:class:`MessageSchema` | `None`
#[pyclass(name = "ServiceSchema", module = "foxglove", get_all, set_all)]
#[derive(Clone)]
pub struct PyServiceSchema {
    /// The name of the service.
    name: String,
    /// The request schema.
    request: Option<PyMessageSchema>,
    /// The response schema.
    response: Option<PyMessageSchema>,
}

#[pymethods]
impl PyServiceSchema {
    #[new]
    #[pyo3(signature = (name, *, request=None, response=None))]
    fn new(
        name: &str,
        request: Option<&PyMessageSchema>,
        response: Option<&PyMessageSchema>,
    ) -> Self {
        PyServiceSchema {
            name: name.to_string(),
            request: request.cloned(),
            response: response.cloned(),
        }
    }
}

impl From<PyServiceSchema> for foxglove::websocket::service::ServiceSchema {
    fn from(value: PyServiceSchema) -> Self {
        let mut schema = foxglove::websocket::service::ServiceSchema::new(value.name);
        if let Some(request) = value.request {
            schema = schema.with_request(request.encoding, request.schema.into());
        }
        if let Some(response) = value.response {
            schema = schema.with_response(response.encoding, response.schema.into());
        }
        schema
    }
}

/// A service request or response schema.
///
/// :param encoding: The encoding of the message.
/// :type encoding: str
/// :param schema: The message schema.
/// :type schema: :py:class:`Schema`
#[pyclass(name = "MessageSchema", module = "foxglove", get_all, set_all)]
#[derive(Clone)]
pub struct PyMessageSchema {
    /// The encoding of the message.
    encoding: String,
    /// The message schema.
    schema: PySchema,
}

#[pymethods]
impl PyMessageSchema {
    #[new]
    #[pyo3(signature = (*, encoding, schema))]
    fn new(encoding: &str, schema: PySchema) -> Self {
        PyMessageSchema {
            encoding: encoding.to_string(),
            schema,
        }
    }
}

/// A Schema is a description of the data format of messages or service calls.
///
/// :param name: The name of the schema.
/// :type name: str
/// :param encoding: The encoding of the schema.
/// :type encoding: str
/// :param data: Schema data.
/// :type data: bytes
#[pyclass(name = "Schema", module = "foxglove", get_all, set_all)]
#[derive(Clone)]
pub struct PySchema {
    /// The name of the schema.
    name: String,
    /// The encoding of the schema.
    encoding: String,
    /// Schema data.
    data: Vec<u8>,
}

#[pymethods]
impl PySchema {
    #[new]
    #[pyo3(signature = (*, name, encoding, data))]
    fn new(name: &str, encoding: &str, data: Vec<u8>) -> Self {
        PySchema {
            name: name.to_string(),
            encoding: encoding.to_string(),
            data,
        }
    }
}

impl From<PySchema> for foxglove::Schema {
    fn from(value: PySchema) -> Self {
        foxglove::Schema::new(value.name, value.encoding, value.data)
    }
}

#[pyclass(name = "ParameterType", module = "foxglove", eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyParameterType {
    /// A byte array, encoded as a base64-encoded string.
    ByteArray,
    /// A decimal or integer value that can be represented as a `float64`.
    Float64,
    /// An array of decimal or integer values that can be represented as `float64`s.
    Float64Array,
}

impl From<PyParameterType> for foxglove::websocket::ParameterType {
    fn from(value: PyParameterType) -> Self {
        match value {
            PyParameterType::ByteArray => foxglove::websocket::ParameterType::ByteArray,
            PyParameterType::Float64 => foxglove::websocket::ParameterType::Float64,
            PyParameterType::Float64Array => foxglove::websocket::ParameterType::Float64Array,
        }
    }
}

impl From<foxglove::websocket::ParameterType> for PyParameterType {
    fn from(value: foxglove::websocket::ParameterType) -> Self {
        match value {
            foxglove::websocket::ParameterType::ByteArray => PyParameterType::ByteArray,
            foxglove::websocket::ParameterType::Float64 => PyParameterType::Float64,
            foxglove::websocket::ParameterType::Float64Array => PyParameterType::Float64Array,
        }
    }
}

/// A parameter value.
#[pyclass(name = "ParameterValue", module = "foxglove")]
#[derive(Clone)]
pub enum PyParameterValue {
    /// A decimal or integer value.
    Number(f64),
    /// A boolean value.
    Bool(bool),
    /// A byte array, which will be encoded as a base64-encoded string.
    Bytes(Vec<u8>),
    /// An array of parameter values.
    Array(Vec<PyParameterValue>),
    /// An associative map of parameter values.
    Dict(HashMap<String, PyParameterValue>),
}

impl From<PyParameterValue> for foxglove::websocket::ParameterValue {
    fn from(value: PyParameterValue) -> Self {
        match value {
            PyParameterValue::Number(n) => foxglove::websocket::ParameterValue::Number(n),
            PyParameterValue::Bool(b) => foxglove::websocket::ParameterValue::Bool(b),
            PyParameterValue::Bytes(items) => foxglove::websocket::ParameterValue::String(items),
            PyParameterValue::Array(py_parameter_values) => {
                foxglove::websocket::ParameterValue::Array(
                    py_parameter_values.into_iter().map(Into::into).collect(),
                )
            }
            PyParameterValue::Dict(hash_map) => foxglove::websocket::ParameterValue::Dict(
                hash_map.into_iter().map(|(k, v)| (k, v.into())).collect(),
            ),
        }
    }
}

impl From<foxglove::websocket::ParameterValue> for PyParameterValue {
    fn from(value: foxglove::websocket::ParameterValue) -> Self {
        match value {
            foxglove::websocket::ParameterValue::Number(n) => PyParameterValue::Number(n),
            foxglove::websocket::ParameterValue::Bool(b) => PyParameterValue::Bool(b),
            foxglove::websocket::ParameterValue::String(items) => PyParameterValue::Bytes(items),
            foxglove::websocket::ParameterValue::Array(parameter_values) => {
                PyParameterValue::Array(parameter_values.into_iter().map(Into::into).collect())
            }
            foxglove::websocket::ParameterValue::Dict(hash_map) => {
                PyParameterValue::Dict(hash_map.into_iter().map(|(k, v)| (k, v.into())).collect())
            }
        }
    }
}

/// A parameter which can be sent to a client.
#[pyclass(name = "Parameter", module = "foxglove")]
#[derive(Clone)]
pub struct PyParameter {
    /// The name of the parameter.
    #[pyo3(get)]
    pub name: String,
    /// The parameter type.
    #[pyo3(get)]
    pub r#type: Option<PyParameterType>,
    /// The parameter value.
    #[pyo3(get)]
    pub value: Option<PyParameterValue>,
}

#[pymethods]
impl PyParameter {
    #[new]
    #[pyo3(signature = (name, *, r#type=None, value=None))]
    pub fn new(
        name: String,
        r#type: Option<PyParameterType>,
        value: Option<PyParameterValue>,
    ) -> Self {
        Self {
            name,
            r#type,
            value,
        }
    }
}

impl From<PyParameter> for foxglove::websocket::Parameter {
    fn from(value: PyParameter) -> Self {
        Self {
            name: value.name,
            r#type: value.r#type.map(Into::into),
            value: value.value.map(Into::into),
        }
    }
}

impl From<foxglove::websocket::Parameter> for PyParameter {
    fn from(value: foxglove::websocket::Parameter) -> Self {
        Self {
            name: value.name,
            r#type: value.r#type.map(Into::into),
            value: value.value.map(Into::into),
        }
    }
}
