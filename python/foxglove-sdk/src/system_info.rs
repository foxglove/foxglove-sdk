use std::time::Duration;

use foxglove::system_info::{SystemInfoHandle, SystemInfoPublisher};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::PyContext;

/// A handle to a running system info publisher.
///
/// The publisher is started by :py:func:`foxglove.start_sysinfo_publisher` and runs in
/// the background until :py:meth:`stop` is called. See :py:func:`foxglove.start_sysinfo_publisher`
/// for the list of metrics published on the channel.
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

/// Start the system info publisher.
///
/// Periodically publishes a ``SystemInfo`` message to a channel containing process and
/// system statistics (memory, CPU, OS info).
///
/// Published metrics
///
/// Each message is a JSON object with a JSON Schema attached to the channel.
/// The following fields are published:
///
/// - ``process_memory`` (number): Resident memory used by the SDK process, in bytes.
/// - ``process_virtual_memory`` (number): Virtual memory used by the SDK process, in bytes.
/// - ``process_cpu_percent`` (number): CPU usage for the SDK process, as a percent of total
///   system CPU capacity (0.0 to 100.0).
/// - ``process_cpu_cores`` (number): CPU usage for the SDK process, expressed in
///   core-equivalents (0.0 to ``num_cpus``). 1.0 means a single logical CPU is fully utilized.
/// - ``total_cpu_percent`` (number): Total CPU usage across all logical CPUs on the system,
///   as a percent (0.0 to 100.0).
/// - ``total_cpu_cores`` (number): Total CPU usage across the system, expressed in
///   core-equivalents (0.0 to ``num_cpus``). 1.0 means one logical CPU's worth of work is being
///   done.
/// - ``num_cpus`` (integer): Number of logical CPUs on the system.
/// - ``total_memory`` (number): Total physical memory on the system, in bytes.
/// - ``used_memory`` (number): Used physical memory on the system, in bytes.
/// - ``total_swap`` (number): Total swap space on the system, in bytes.
/// - ``used_swap`` (number): Used swap space on the system, in bytes.
/// - ``kernel_version`` (string): Kernel version string, or empty if unknown.
/// - ``os_version`` (string): OS version string, or empty if unknown.
///
/// CPU usage values are computed from the difference between consecutive samples, so they
/// reflect activity over the most recent refresh interval.
///
/// :param topic: The channel topic name. Defaults to ``/sysinfo``.
/// :type topic: str | None
/// :param refresh_interval: How often to publish, in seconds. Defaults to ``0.5``.
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
