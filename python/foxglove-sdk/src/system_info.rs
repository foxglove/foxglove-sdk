use std::time::Duration;

use foxglove::system_info::{SystemInfoHandle, SystemInfoPublisher};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::PyContext;

/// A handle to a running system info publisher.
///
/// The publisher is started by :py:func:`foxglove.start_sysinfo_publisher` and runs in
/// the background until :py:meth:`stop` is called or this object is garbage collected.
#[pyclass(name = "SystemInfoPublisher", module = "foxglove")]
pub struct PySystemInfoPublisher(Option<SystemInfoHandle>);

#[pymethods]
impl PySystemInfoPublisher {
    /// Stop the publisher.
    ///
    /// Aborts the background task. Subsequent calls to ``stop`` are no-ops.
    pub fn stop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

impl Drop for PySystemInfoPublisher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start the system info publisher.
///
/// Periodically publishes a ``SystemInfo`` message to a channel containing process and
/// system statistics (memory, CPU, OS info).
///
/// :param topic: The channel topic name. Defaults to ``/sysinfo``.
/// :type topic: str | None
/// :param refresh_interval: How often to publish, in seconds. Defaults to ``1.0``.
///     Clamped to a minimum of 200ms.
/// :type refresh_interval: float | None
/// :param context: The context on which the publisher creates its channel. Defaults to
///     the global default context.
/// :type context: :py:class:`Context` | None
/// :return: A handle that can be used to stop the publisher.
/// :rtype: :py:class:`SystemInfoPublisher`
#[pyfunction]
#[pyo3(signature = (*, topic=None, refresh_interval=None, context=None))]
pub fn start_sysinfo_publisher(
    topic: Option<String>,
    refresh_interval: Option<f64>,
    context: Option<PyRef<PyContext>>,
) -> PyResult<PySystemInfoPublisher> {
    let mut builder = SystemInfoPublisher::new();

    if let Some(topic) = topic {
        builder = builder.topic(topic);
    }

    if let Some(seconds) = refresh_interval {
        let duration = Duration::try_from_secs_f64(seconds).map_err(|_| {
            PyValueError::new_err(format!(
                "refresh_interval must be a non-negative finite number, got {seconds}"
            ))
        })?;
        builder = builder.refresh_interval(duration);
    }

    if let Some(context) = context {
        builder = builder.context(&context.0);
    }

    Ok(PySystemInfoPublisher(Some(builder.start())))
}
