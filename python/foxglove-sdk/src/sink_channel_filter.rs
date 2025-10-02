use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
use pyo3::{
    types::{PyDict, PyInt, PyString},
    Py,
};

/// Information about a channel, which is passed to a :py:class:`SinkChannelFilter`.
///
/// This is a view into a :py:class:`Channel`.
#[pyclass(name = "FilterableChannel", module = "foxglove")]
pub struct PyFilterableChannel {
    #[pyo3(get)]
    id: Py<PyInt>,
    #[pyo3(get)]
    topic: Py<PyString>,
    #[pyo3(get)]
    metadata: Py<PyDict>,
}

#[pymethods]
impl PyFilterableChannel {
    fn __repr__(&self) -> String {
        format!(
            "FilterableChannel(id={}, topic='{}', metadata='{:?}')",
            self.id, self.topic, self.metadata
        )
    }
}

/// A filter for channels that can be used to subscribe to or unsubscribe from channels.
///
/// This can be used to omit one or more channels from a sink, but still log all channels to another
/// sink in the same context.
///
/// Return `True` to log the channel, or `False` to skip it.
#[pyclass(name = "SinkChannelFilter", module = "foxglove")]
pub struct PySinkChannelFilter(pub Arc<Py<PyAny>>);
impl foxglove::SinkChannelFilter for PySinkChannelFilter {
    fn should_subscribe(&self, channel: &dyn foxglove::FilterableChannel) -> bool {
        let handler = self.0.clone();
        Python::with_gil(|py| {
            let metadata = channel.metadata().into_py_dict(py).unwrap_or_else(|err| {
                tracing::error!("Failed to constrcut channel metadata: {}", err.to_string());
                PyDict::new(py)
            });
            let channel = PyFilterableChannel {
                id: PyInt::new(py, u64::from(channel.id())).into(),
                topic: PyString::new(py, channel.topic()).into(),
                metadata: metadata.into(),
            };
            let result = handler
                .bind(py)
                .call((channel,), None)
                .and_then(|f| f.extract::<bool>());

            match result {
                Ok(result) => result,
                Err(err) => {
                    tracing::error!("Error in SinkChannelFilter: {}", err.to_string());
                    false
                }
            }
        })
    }
}
