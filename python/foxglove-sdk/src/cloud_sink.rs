use crate::websocket::PyServerListener;
use crate::{errors::PyFoxgloveError, PyContext};
use foxglove::{websocket::ServerListener, CloudSink, CloudSinkHandle, CloudSinkListener};
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass(name = "CloudSinkListener", module = "foxglove")]
pub struct PyCloudSinkListener(PyServerListener);

impl PyCloudSinkListener {
    pub(crate) fn new(listener: Py<PyAny>) -> Self {
        Self(PyServerListener::new(listener))
    }
}

impl CloudSinkListener for PyCloudSinkListener {
    fn on_message_data(
        &self,
        client: foxglove::websocket::Client,
        client_channel: &foxglove::websocket::ClientChannel,
        payload: &[u8],
    ) {
        self.0.on_message_data(client, client_channel, payload);
    }

    fn on_subscribe(
        &self,
        client: foxglove::websocket::Client,
        channel: foxglove::websocket::ChannelView,
    ) {
        self.0.on_subscribe(client, channel);
    }

    fn on_unsubscribe(
        &self,
        client: foxglove::websocket::Client,
        channel: foxglove::websocket::ChannelView,
    ) {
        self.0.on_unsubscribe(client, channel);
    }

    fn on_client_advertise(
        &self,
        client: foxglove::websocket::Client,
        channel: &foxglove::websocket::ClientChannel,
    ) {
        self.0.on_client_advertise(client, channel);
    }

    fn on_client_unadvertise(
        &self,
        client: foxglove::websocket::Client,
        channel: &foxglove::websocket::ClientChannel,
    ) {
        self.0.on_client_unadvertise(client, channel);
    }
}

/// An CloudSink for live visualization and teleop in Foxglove.
///
/// Must run Foxglove Agent on the same host for this to work.
#[pyfunction]
#[pyo3(signature = (*, listener=None, supported_encodings=None, context=None, session_id=None))]
#[allow(clippy::too_many_arguments)]
pub fn start_cloud_sink(
    py: Python<'_>,
    listener: Option<Py<PyAny>>,
    supported_encodings: Option<Vec<String>>,
    context: Option<PyRef<PyContext>>,
    session_id: Option<String>,
) -> PyResult<PyCloudSink> {
    let mut cloud = CloudSink::new();

    if let Some(session_id) = session_id {
        cloud = cloud.session_id(session_id);
    }

    if let Some(py_obj) = listener {
        let listener = PyCloudSinkListener::new(py_obj);
        cloud = cloud.listener(Arc::new(listener));
    }

    if let Some(supported_encodings) = supported_encodings {
        cloud = cloud.supported_encodings(supported_encodings);
    }

    if let Some(context) = context {
        cloud = cloud.context(&context.0);
    }

    let handle = py
        .allow_threads(|| cloud.start_blocking())
        .map_err(PyFoxgloveError::from)?;

    Ok(PyCloudSink(Some(handle)))
}

/// A handle to the CloudSink connection.
///
/// This handle can safely be dropped and the connection will run forever.
#[pyclass(name = "CloudSink", module = "foxglove")]
pub struct PyCloudSink(pub Option<CloudSinkHandle>);

#[pymethods]
impl PyCloudSink {
    /// Gracefully disconnect from the agent.
    ///
    /// If the agent has already been disconnected, this has no effect.
    pub fn stop(&mut self, py: Python<'_>) {
        if let Some(agent) = self.0.take() {
            if let Some(shutdown) = agent.stop() {
                py.allow_threads(|| shutdown.wait_blocking());
            }
        }
    }
}

pub fn register_submodule(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let module = PyModule::new(parent_module.py(), "cloud")?;

    module.add_class::<PyCloudSink>()?;
    module.add_class::<PyCloudSinkListener>()?;

    // Define as a package
    // https://github.com/PyO3/pyo3/issues/759
    let py = parent_module.py();
    py.import("sys")?
        .getattr("modules")?
        .set_item("foxglove._foxglove_py.cloud", &module)?;

    parent_module.add_submodule(&module)
}
