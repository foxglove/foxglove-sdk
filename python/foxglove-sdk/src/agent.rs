use crate::websocket::{PyCapability, PyServerListener};
use crate::{errors::PyFoxgloveError, PyContext};
use foxglove::{Agent, AgentHandle};
use pyo3::prelude::*;
use std::sync::Arc;

/// Connect to Foxglove Agent for live visualization and teleop.
///
/// Foxglove Agent must be running on the same host for this to work.
#[pyfunction]
#[pyo3(signature = (*, capabilities=None, server_listener=None, supported_encodings=None, context=None, session_id=None))]
#[allow(clippy::too_many_arguments)]
pub fn connect_agent(
    py: Python<'_>,
    capabilities: Option<Vec<PyCapability>>,
    server_listener: Option<Py<PyAny>>,
    supported_encodings: Option<Vec<String>>,
    context: Option<PyRef<PyContext>>,
    session_id: Option<String>,
) -> PyResult<PyAgent> {
    let mut agent = Agent::new();

    if let Some(session_id) = session_id {
        agent = agent.session_id(session_id);
    }

    if let Some(py_obj) = server_listener {
        let listener = PyServerListener::new(py_obj);
        agent = agent.listener(Arc::new(listener));
    }

    if let Some(capabilities) = capabilities {
        agent = agent.capabilities(capabilities.into_iter().map(PyCapability::into));
    }

    if let Some(supported_encodings) = supported_encodings {
        agent = agent.supported_encodings(supported_encodings);
    }

    if let Some(context) = context {
        agent = agent.context(&context.0);
    }

    let handle = py
        .allow_threads(|| agent.connect())
        .map_err(PyFoxgloveError::from)?;

    Ok(PyAgent(Some(handle)))
}

/// A handle to the Agent Remote Connection.
///
/// Obtain an instance by calling :py:func:`foxglove.connect_agent`.
#[pyclass(name = "Agent", module = "foxglove")]
pub struct PyAgent(pub Option<AgentHandle>);

#[pymethods]
impl PyAgent {
    /// Gracefully disconnect from the agent.
    ///
    /// If the agent has already been disconnected, this has no effect.
    pub fn disconnect(&mut self, py: Python<'_>) {
        if let Some(agent) = self.0.take() {
            if let Some(shutdown) = agent.disconnect() {
                py.allow_threads(|| shutdown.wait_blocking());
            }
        }
    }
}

pub fn register_submodule(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let module = PyModule::new(parent_module.py(), "agent")?;

    module.add_class::<PyAgent>()?;
    module.add_function(wrap_pyfunction!(connect_agent, &module)?)?;

    // Define as a package
    // https://github.com/PyO3/pyo3/issues/759
    let py = parent_module.py();
    py.import("sys")?
        .getattr("modules")?
        .set_item("foxglove._foxglove_py.agent", &module)?;

    parent_module.add_submodule(&module)
}
