use foxglove::ChannelDescriptor;
use pyo3::prelude::*;
use pyo3::types::IntoPyDict;
use pyo3::{types::PyDict, Py};

use crate::PySchema;

/// Information about a Channel.
#[pyclass(name = "ChannelDescriptor", module = "foxglove")]
pub struct PyChannelDescriptor(ChannelDescriptor);

#[pymethods]
impl PyChannelDescriptor {
    /// Returns the channel ID.
    #[getter]
    fn id(&self) -> u64 {
        u64::from(self.0.id())
    }

    /// Returns the channel topic.
    #[getter]
    fn topic(&self) -> &str {
        self.0.topic()
    }

    /// Returns the message encoding for this channel.
    #[getter]
    fn message_encoding(&self) -> &str {
        self.0.message_encoding()
    }

    /// Returns the metadata for this channel.
    #[getter]
    fn metadata<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let metadata = self.0.metadata().into_py_dict(py).unwrap_or_else(|err| {
            tracing::error!("Failed to construct channel metadata: {}", err.to_string());
            PyDict::new(py)
        });
        Ok(metadata)
    }

    /// Returns the schema for this channel.
    #[getter]
    fn schema(&self) -> Option<PySchema> {
        self.0.schema().map(|schema| PySchema::from(schema.clone()))
    }

    fn __repr__(&self) -> String {
        format!(
            "ChannelDescriptor(id={}, topic='{}')",
            self.0.id(),
            self.0.topic(),
        )
    }
}

/// A filter for channels that can be used to subscribe to or unsubscribe from channels.
///
/// This can be used to omit one or more channels from a sink, but still log all channels to another
/// sink in the same context.
///
/// Return ``True`` to log the channel, or ``False`` to skip it. If the callback errors, then
/// the channel will be skipped.
#[pyclass(name = "SinkChannelFilter", module = "foxglove")]
pub struct PySinkChannelFilter(pub Py<PyAny>);

impl foxglove::SinkChannelFilter for PySinkChannelFilter {
    fn should_subscribe(&self, channel: &ChannelDescriptor) -> bool {
        Python::with_gil(|py| {
            let handler = self.0.clone_ref(py);
            let descriptor = PyChannelDescriptor(channel.clone());
            let result = handler
                .bind(py)
                .call((descriptor,), None)
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
