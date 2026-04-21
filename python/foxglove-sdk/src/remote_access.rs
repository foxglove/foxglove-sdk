use std::sync::Arc;
use std::time::Duration;

use foxglove::ChannelDescriptor;
use foxglove::remote_access::{
    self, Capability, ConnectionStatus, Gateway, GatewayHandle, Listener, QosProfile, Reliability,
    Status,
};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::PyContext;
use crate::errors::PyFoxgloveError;
use crate::remote_common::{PyParameter, PyService, PyStatusLevel};
use crate::sink_channel_filter::{PyChannelDescriptor, PySinkChannelFilter};

/// A client connected to a running remote access gateway.
#[pyclass(name = "Client", module = "foxglove.remote_access")]
pub struct PyRemoteAccessClient {
    #[pyo3(get)]
    id: u32,
}

#[pymethods]
impl PyRemoteAccessClient {
    fn __repr__(&self) -> String {
        format!("Client(id={})", self.id)
    }
}

impl From<&remote_access::Client> for PyRemoteAccessClient {
    fn from(value: &remote_access::Client) -> Self {
        Self {
            id: value.id().into(),
        }
    }
}

/// The status of the remote access gateway connection.
#[pyclass(
    name = "RemoteAccessConnectionStatus",
    module = "foxglove.remote_access",
    eq,
    eq_int
)]
#[derive(Clone, PartialEq)]
#[repr(u8)]
pub enum PyConnectionStatus {
    /// The gateway is attempting to establish or re-establish a connection.
    Connecting = 0,
    /// The gateway is connected and handling events.
    Connected = 1,
    /// The gateway is shutting down. Listener callbacks may still be in progress.
    ShuttingDown = 2,
    /// The gateway has been shut down. No further listener callbacks will be invoked.
    Shutdown = 3,
}

impl From<ConnectionStatus> for PyConnectionStatus {
    fn from(value: ConnectionStatus) -> Self {
        match value {
            ConnectionStatus::Connecting => PyConnectionStatus::Connecting,
            ConnectionStatus::Connected => PyConnectionStatus::Connected,
            ConnectionStatus::ShuttingDown => PyConnectionStatus::ShuttingDown,
            ConnectionStatus::Shutdown => PyConnectionStatus::Shutdown,
        }
    }
}

/// A capability that can be advertised by a remote access gateway.
#[pyclass(name = "Capability", module = "foxglove.remote_access", eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyRemoteAccessCapability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
    /// Allow clients to get, set, and subscribe to parameter updates.
    Parameters,
    /// Allow clients to call services.
    Services,
}

impl From<PyRemoteAccessCapability> for Capability {
    fn from(value: PyRemoteAccessCapability) -> Self {
        match value {
            PyRemoteAccessCapability::ClientPublish => Capability::ClientPublish,
            PyRemoteAccessCapability::Parameters => Capability::Parameters,
            PyRemoteAccessCapability::Services => Capability::Services,
        }
    }
}

/// A mechanism to register callbacks for handling remote access client events.
///
/// Wraps a Python object implementing the RemoteAccessListener protocol.
pub struct PyRemoteAccessListener {
    listener: Py<PyAny>,
}

impl Listener for PyRemoteAccessListener {
    fn on_connection_status_changed(&self, status: ConnectionStatus) {
        let result: PyResult<()> = Python::with_gil(|py| {
            let py_status = PyConnectionStatus::from(status);
            self.listener.bind(py).call_method(
                "on_connection_status_changed",
                (py_status,),
                None,
            )?;
            Ok(())
        });
        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err);
        }
    }

    fn on_subscribe(&self, client: &remote_access::Client, channel: &ChannelDescriptor) {
        self.call_client_channel_method("on_subscribe", client, channel);
    }

    fn on_unsubscribe(&self, client: &remote_access::Client, channel: &ChannelDescriptor) {
        self.call_client_channel_method("on_unsubscribe", client, channel);
    }

    fn on_client_advertise(&self, client: &remote_access::Client, channel: &ChannelDescriptor) {
        self.call_client_channel_method("on_client_advertise", client, channel);
    }

    fn on_client_unadvertise(&self, client: &remote_access::Client, channel: &ChannelDescriptor) {
        self.call_client_channel_method("on_client_unadvertise", client, channel);
    }

    fn on_message_data(
        &self,
        client: &remote_access::Client,
        channel: &ChannelDescriptor,
        payload: &[u8],
    ) {
        let py_client = PyRemoteAccessClient::from(client);
        let result: PyResult<()> = Python::with_gil(|py| {
            let py_channel = PyChannelDescriptor(channel.clone());
            let py_payload = PyBytes::new(py, payload);
            self.listener.bind(py).call_method(
                "on_message_data",
                (py_client, py_channel, py_payload),
                None,
            )?;
            Ok(())
        });
        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err);
        }
    }

    fn on_get_parameters(
        &self,
        client: &remote_access::Client,
        param_names: Vec<String>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::remote_access::Parameter> {
        let py_client = PyRemoteAccessClient::from(client);
        let result: PyResult<Vec<foxglove::remote_access::Parameter>> = Python::with_gil(|py| {
            let args = (py_client, param_names, request_id);
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
                tracing::error!("Callback failed: {}", err);
                vec![]
            }
        }
    }

    fn on_set_parameters(
        &self,
        client: &remote_access::Client,
        parameters: Vec<foxglove::remote_access::Parameter>,
        request_id: Option<&str>,
    ) -> Vec<foxglove::remote_access::Parameter> {
        let py_client = PyRemoteAccessClient::from(client);
        let result: PyResult<Vec<foxglove::remote_access::Parameter>> = Python::with_gil(|py| {
            let parameters: Vec<PyParameter> = parameters.into_iter().map(Into::into).collect();
            let args = (py_client, parameters, request_id);
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
                tracing::error!("Callback failed: {}", err);
                vec![]
            }
        }
    }

    fn on_parameters_subscribe(&self, param_names: Vec<String>) {
        let result: PyResult<()> = Python::with_gil(|py| {
            self.listener
                .bind(py)
                .call_method("on_parameters_subscribe", (param_names,), None)?;
            Ok(())
        });
        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err);
        }
    }

    fn on_parameters_unsubscribe(&self, param_names: Vec<String>) {
        let result: PyResult<()> = Python::with_gil(|py| {
            self.listener.bind(py).call_method(
                "on_parameters_unsubscribe",
                (param_names,),
                None,
            )?;
            Ok(())
        });
        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err);
        }
    }
}

impl PyRemoteAccessListener {
    fn call_client_channel_method(
        &self,
        method_name: &str,
        client: &remote_access::Client,
        channel: &ChannelDescriptor,
    ) {
        let py_client = PyRemoteAccessClient::from(client);
        let result: PyResult<()> = Python::with_gil(|py| {
            let py_channel = PyChannelDescriptor(channel.clone());
            self.listener
                .bind(py)
                .call_method(method_name, (py_client, py_channel), None)?;
            Ok(())
        });
        if let Err(err) = result {
            tracing::error!("Callback failed: {}", err);
        }
    }
}

/// A handle to a running remote access gateway.
///
/// Obtain an instance by calling :py:func:`foxglove.start_gateway`.
#[pyclass(name = "RemoteAccessGateway", module = "foxglove")]
pub struct PyRemoteAccessGateway(Option<GatewayHandle>);

#[pymethods]
impl PyRemoteAccessGateway {
    /// Returns the current connection status.
    pub fn connection_status(&self) -> PyConnectionStatus {
        self.0
            .as_ref()
            .map(|h| h.connection_status().into())
            .unwrap_or(PyConnectionStatus::Shutdown)
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
        if let Some(handle) = &self.0 {
            py.allow_threads(move || {
                handle
                    .add_services(services.into_iter().map(Into::into))
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
        if let Some(handle) = &self.0 {
            py.allow_threads(move || handle.remove_services(names));
        }
    }

    /// Publishes parameter values to all subscribed clients.
    ///
    /// :param parameters: The parameters to publish.
    /// :type parameters: list[Parameter]
    pub fn publish_parameter_values(&self, parameters: Vec<PyParameter>) {
        if let Some(handle) = &self.0 {
            handle.publish_parameter_values(parameters.into_iter().map(Into::into).collect());
        }
    }

    /// Send a status message to all connected participants.
    ///
    /// :param message: The message to send.
    /// :type message: str
    /// :param level: The level of the status message.
    /// :type level: StatusLevel
    /// :param id: An optional ID for the status message.
    /// :type id: str | None
    #[pyo3(signature = (message, level, id=None))]
    pub fn publish_status(&self, message: String, level: &PyStatusLevel, id: Option<String>) {
        if let Some(handle) = &self.0 {
            let status = match id {
                Some(id) => Status::new(level.clone().into(), message).with_id(id),
                None => Status::new(level.clone().into(), message),
            };
            handle.publish_status(status);
        }
    }

    /// Remove status messages by ID from all connected participants.
    ///
    /// :param ids: The IDs of the status messages to remove.
    /// :type ids: list[str]
    pub fn remove_status(&self, ids: Vec<String>) {
        if let Some(handle) = &self.0 {
            handle.remove_status(ids);
        }
    }

    /// Gracefully disconnect from the remote access gateway.
    pub fn stop(&mut self, py: Python<'_>) {
        if let Some(handle) = self.0.take() {
            py.allow_threads(|| handle.stop_blocking())
        }
    }
}

/// The reliability policy for a channel's data delivery.
#[pyclass(name = "Reliability", module = "foxglove.remote_access", eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyReliability {
    /// Data is sent over unreliable data tracks. This is the default.
    Lossy,
    /// Data is sent over the reliable control channel (ordered, guaranteed delivery).
    Reliable,
}

impl From<PyReliability> for Reliability {
    fn from(value: PyReliability) -> Self {
        match value {
            PyReliability::Lossy => Reliability::Lossy,
            PyReliability::Reliable => Reliability::Reliable,
        }
    }
}

/// Quality-of-service profile for a channel.
#[pyclass(name = "QosProfile", module = "foxglove.remote_access")]
#[derive(Clone)]
pub struct PyQosProfile {
    #[pyo3(get, set)]
    pub reliability: PyReliability,
}

#[pymethods]
impl PyQosProfile {
    #[new]
    #[pyo3(signature = (*, reliability=PyReliability::Lossy))]
    fn new(reliability: PyReliability) -> Self {
        Self { reliability }
    }
}

impl From<PyQosProfile> for QosProfile {
    fn from(value: PyQosProfile) -> Self {
        QosProfile::builder()
            .reliability(value.reliability.into())
            .build()
    }
}

/// A QoS classifier wrapping a Python callable.
///
/// The callable should accept a `ChannelDescriptor` and return a `QosProfile`.
pub struct PyQosClassifier(pub Py<PyAny>);

impl foxglove::remote_access::QosClassifier for PyQosClassifier {
    fn classify(&self, channel: &foxglove::ChannelDescriptor) -> QosProfile {
        Python::with_gil(|py| {
            let handler = self.0.clone_ref(py);
            let descriptor = PyChannelDescriptor(channel.clone());
            let result = handler
                .bind(py)
                .call((descriptor,), None)
                .and_then(|f| f.extract::<PyQosProfile>());

            match result {
                Ok(profile) => profile.into(),
                Err(err) => {
                    tracing::error!("Error in QoS classifier: {}", err.to_string());
                    QosProfile::default()
                }
            }
        })
    }
}

/// Start a remote access gateway for live visualization and teleop in Foxglove.
#[pyfunction]
#[pyo3(signature = (*, name=None, device_token=None, capabilities=None, listener=None, supported_encodings=None, services=None, context=None, channel_filter=None, qos_classifier=None, message_backlog_size=None, foxglove_api_url=None, foxglove_api_timeout=None, sysinfo_refresh_interval=None))]
#[allow(clippy::too_many_arguments)]
pub fn start_gateway(
    py: Python<'_>,
    name: Option<String>,
    device_token: Option<String>,
    capabilities: Option<Vec<PyRemoteAccessCapability>>,
    listener: Option<Py<PyAny>>,
    supported_encodings: Option<Vec<String>>,
    services: Option<Vec<PyService>>,
    context: Option<PyRef<PyContext>>,
    channel_filter: Option<Py<PyAny>>,
    qos_classifier: Option<Py<PyAny>>,
    message_backlog_size: Option<usize>,
    foxglove_api_url: Option<String>,
    foxglove_api_timeout: Option<f64>,
    sysinfo_refresh_interval: Option<f64>,
) -> PyResult<PyRemoteAccessGateway> {
    let mut gateway = Gateway::new();

    if let Some(name) = name {
        gateway = gateway.name(name);
    }

    if let Some(token) = device_token {
        gateway = gateway.device_token(token);
    }

    if let Some(capabilities) = capabilities {
        gateway = gateway.capabilities(capabilities.into_iter().map(Into::into));
    }

    if let Some(py_obj) = listener {
        let listener = PyRemoteAccessListener { listener: py_obj };
        gateway = gateway.listener(Arc::new(listener));
    }

    if let Some(supported_encodings) = supported_encodings {
        gateway = gateway.supported_encodings(supported_encodings);
    }

    if let Some(services) = services {
        gateway = gateway.services(services.into_iter().map(Into::into));
    }

    if let Some(context) = context {
        gateway = gateway.context(&context.0);
    }

    if let Some(channel_filter) = channel_filter {
        gateway = gateway.channel_filter(Arc::new(PySinkChannelFilter(channel_filter)));
    }

    if let Some(qos_classifier) = qos_classifier {
        gateway = gateway.qos_classifier(Arc::new(PyQosClassifier(qos_classifier)));
    }

    if let Some(size) = message_backlog_size {
        gateway = gateway.message_backlog_size(size);
    }

    if let Some(url) = foxglove_api_url {
        gateway = gateway.foxglove_api_url(url);
    }

    if let Some(timeout) = foxglove_api_timeout {
        let duration = Duration::try_from_secs_f64(timeout).map_err(|_| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "foxglove_api_timeout must be a non-negative finite number, got {timeout}"
            ))
        })?;
        gateway = gateway.foxglove_api_timeout(duration);
    }

    if let Some(seconds) = sysinfo_refresh_interval {
        let duration = Duration::try_from_secs_f64(seconds).map_err(|_| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "sysinfo_refresh_interval must be a non-negative finite number, got {seconds}"
            ))
        })?;
        gateway = gateway.sysinfo(Some(duration));
    }

    let handle = py
        .allow_threads(|| gateway.start())
        .map_err(PyFoxgloveError::from)?;

    Ok(PyRemoteAccessGateway(Some(handle)))
}

pub fn register_submodule(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let module = PyModule::new(parent_module.py(), "remote_access")?;

    module.add_class::<PyRemoteAccessGateway>()?;
    module.add_class::<PyRemoteAccessCapability>()?;
    module.add_class::<PyRemoteAccessClient>()?;
    module.add_class::<PyConnectionStatus>()?;
    module.add_class::<PyReliability>()?;
    module.add_class::<PyQosProfile>()?;

    let py = parent_module.py();
    py.import("sys")?
        .getattr("modules")?
        .set_item("foxglove._foxglove_py.remote_access", &module)?;

    parent_module.add_submodule(&module)
}
