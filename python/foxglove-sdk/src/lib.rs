use errors::PyFoxgloveError;
use foxglove::{
    Channel, ChannelBuilder, LogContext, McapWriter, McapWriterHandle, Schema, WebSocketServer,
    WebSocketServerBlockingHandle,
};
use log::LevelFilter;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::sync::Arc;
use std::time;

mod errors;
mod generated;

use generated::channels;
use generated::schemas;

#[pyclass]
struct BaseChannel(Arc<Channel>);

/// A live visualization server. Obtain an instance by calling :py:func:`start_server`.
#[pyclass(name = "WebSocketServer")]
struct PyWebSocketServer(Option<WebSocketServerBlockingHandle>);

#[pymethods]
impl PyWebSocketServer {
    fn stop(&mut self, py: Python<'_>) {
        if let Some(server) = self.0.take() {
            py.allow_threads(|| server.stop())
        }
    }

    /// Sets a new session ID and notifies all clients, causing them to reset their state.
    /// If no session ID is provided, generates a new one based on the current timestamp.
    #[pyo3(signature = (session_id=None))]
    fn clear_session(&self, session_id: Option<String>) -> PyResult<()> {
        if let Some(server) = &self.0 {
            server.clear_session(session_id);
        }
        Ok(())
    }

    fn broadcast_time(&self, timestamp_nanos: u64) -> PyResult<()> {
        if let Some(server) = &self.0 {
            server.broadcast_time(timestamp_nanos);
        }
        Ok(())
    }
}

/// A capability that the websocket server advertises to its clients.
#[pyclass(eq, eq_int)]
#[derive(Clone, PartialEq)]
enum Capability {
    /// Allow clients to advertise channels to send data messages to the server.
    // ClientPublish,
    /// Allow clients to get & set parameters.
    // Parameters,
    /// Inform clients about the latest server time.
    ///
    /// This allows accelerated, slowed, or stepped control over the progress of time. If the
    /// server publishes time data, then timestamps of published messages must originate from the
    /// same time source.
    Time,
}

impl From<Capability> for foxglove::websocket::Capability {
    fn from(value: Capability) -> Self {
        match value {
            // Capability::ClientPublish => foxglove::websocket::Capability::ClientPublish,
            // Capability::Parameters => foxglove::websocket::Capability::Parameters,
            Capability::Time => foxglove::websocket::Capability::Time,
        }
    }
}

///  A writer for logging messages to an MCAP file.
///
/// Obtain an instance by calling :py:func:`record_file`, or the context-managed
/// :py:func:`new_mcap_file`.
///
/// If you're using :py:func:`record_file`, you must maintain a reference to the returned writer
/// until you are done logging. The writer will be closed automatically when it is garbage
/// collected, but you may also :py:func:`MCAPWriter.close` it explicitly.
#[pyclass(name = "MCAPWriter")]
struct PyMcapWriter(Option<McapWriterHandle<BufWriter<File>>>);

impl Drop for PyMcapWriter {
    fn drop(&mut self) {
        log::info!("MCAP writer dropped");
        if let Err(e) = self.close() {
            log::error!("Failed to close MCAP writer: {e}");
        }
    }
}

#[pymethods]
impl PyMcapWriter {
    /// Close the MCAP writer.
    ///
    /// You may call this to explicitly close the writer. Note that the writer will be automatically
    /// closed for you when it is garbage collected, or when using the context-managed
    /// :py:func:`new_mcap_file`.
    fn close(&mut self) -> PyResult<()> {
        if let Some(writer) = self.0.take() {
            writer.close().map_err(PyFoxgloveError::from)?;
        }
        Ok(())
    }
}

#[pymethods]
impl BaseChannel {
    #[new]
    #[pyo3(
        signature = (topic, message_encoding, schema_name, schema_encoding=None, schema_data=None, metadata=None)
    )]
    fn new(
        topic: &str,
        message_encoding: &str,
        schema_name: Option<String>,
        schema_encoding: Option<String>,
        schema_data: Option<Vec<u8>>,
        metadata: Option<BTreeMap<String, String>>,
    ) -> PyResult<Self> {
        let schema = match (
            schema_name,
            schema_encoding.filter(|s| !s.is_empty()),
            schema_data.filter(|s| !s.is_empty()),
        ) {
            (Some(name), Some(encoding), Some(data)) => Some(Schema::new(name, encoding, data)),
            (_, None, None) => None,
            (_, None, Some(_)) => {
                return Err(PyValueError::new_err(
                    "Schema encoding must be provided if schema data is provided.",
                ));
            }
            (_, Some(_), None) => {
                return Err(PyValueError::new_err(
                    "Schema data must be provided if schema encoding is provided.",
                ));
            }
            _ => {
                return Err(PyValueError::new_err(
                    "Schema name must be provided if schema data or encoding is provided.",
                ));
            }
        };

        let channel = ChannelBuilder::new(topic)
            .message_encoding(message_encoding)
            .schema(schema)
            .metadata(metadata.unwrap_or_default())
            .build()
            .map_err(PyFoxgloveError::from)?;

        Ok(BaseChannel(channel))
    }

    #[pyo3(signature = (msg, publish_time=None, log_time=None, sequence=None))]
    fn log(
        &self,
        msg: &[u8],
        publish_time: Option<u64>,
        log_time: Option<u64>,
        sequence: Option<u32>,
    ) -> PyResult<()> {
        let metadata = foxglove::PartialMetadata {
            sequence,
            log_time,
            publish_time,
        };
        self.0.log_with_meta(msg, metadata);
        Ok(())
    }
}

#[pyclass]
#[derive(Clone, Default)]
struct PartialMetadata(foxglove::PartialMetadata);

#[pymethods]
impl PartialMetadata {
    #[new]
    #[pyo3(signature = (sequence=None, log_time=None, publish_time=None))]
    fn new(sequence: Option<u32>, log_time: Option<u64>, publish_time: Option<u64>) -> Self {
        Self(foxglove::PartialMetadata {
            sequence,
            log_time,
            publish_time,
        })
    }
}

impl From<PartialMetadata> for foxglove::PartialMetadata {
    fn from(value: PartialMetadata) -> Self {
        value.0
    }
}

/// Open a new mcap file for recording.
///
/// :param path: The path to the MCAP file. This file will be created and must not already exist.
/// :return: A new `MCAPWriter` object.
#[pyfunction]
#[pyo3(signature = (path))]
fn record_file(path: &str) -> PyResult<PyMcapWriter> {
    let handle = McapWriter::new()
        .create_new_buffered_file(path)
        .map_err(PyFoxgloveError::from)?;
    Ok(PyMcapWriter(Some(handle)))
}

/// Start a new Foxglove WebSocket server.
///
/// :param name: The name of the server.
/// :param host: The host to bind to.
/// :param port: The port to bind to.
///
/// To connect to this server: open Foxglove, choose "Open a new connection", and select Foxglove
/// WebSocket. The default connection string matches the defaults used by the SDK.
#[pyfunction]
#[pyo3(signature = (*, name = None, host="127.0.0.1", port=8765, capabilities=None))]
fn start_server(
    py: Python<'_>,
    name: Option<String>,
    host: &str,
    port: u16,
    capabilities: Option<Vec<Capability>>,
) -> PyResult<PyWebSocketServer> {
    let session_id = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .expect("Failed to create session ID; invalid system time")
        .as_millis()
        .to_string();

    let mut server = WebSocketServer::new()
        .session_id(session_id)
        .bind(host, port);

    if let Some(capabilities) = capabilities {
        server = server.capabilities(capabilities.into_iter().map(Capability::into));
    }

    if let Some(name) = name {
        server = server.name(name);
    }

    let handle = py
        .allow_threads(|| server.start_blocking())
        .map_err(PyFoxgloveError::from)?;

    Ok(PyWebSocketServer(Some(handle)))
}

#[pyfunction]
fn get_channel_for_topic(topic: &str) -> PyResult<Option<BaseChannel>> {
    let channel = LogContext::global().get_channel_by_topic(topic);
    Ok(channel.map(BaseChannel))
}

// Not public. Re-exported in a wrapping function.
#[pyfunction]
fn enable_logging(level: &str) -> PyResult<()> {
    let level = match level.to_lowercase().as_str() {
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        _ => return Err(PyErr::new::<PyValueError, _>("Invalid log level")),
    };

    log::set_max_level(level);
    Ok(())
}

// Not public. Re-exported in a wrapping function.
#[pyfunction]
fn disable_logging() -> PyResult<()> {
    log::set_max_level(LevelFilter::Off);
    Ok(())
}

// Not public. Registered as an atexit handler.
#[pyfunction]
fn shutdown(py: Python<'_>) {
    py.allow_threads(foxglove::shutdown_runtime);
}

/// Our public API is in the `python` directory.
/// Rust bindings are exported as `_foxglove_py` and should not be imported directly.
#[pymodule]
fn _foxglove_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    pyo3_log::init();
    m.add_function(wrap_pyfunction!(enable_logging, m)?)?;
    m.add_function(wrap_pyfunction!(disable_logging, m)?)?;
    m.add_function(wrap_pyfunction!(shutdown, m)?)?;
    m.add_function(wrap_pyfunction!(record_file, m)?)?;
    m.add_function(wrap_pyfunction!(start_server, m)?)?;
    m.add_function(wrap_pyfunction!(get_channel_for_topic, m)?)?;
    m.add_class::<BaseChannel>()?;
    m.add_class::<PyWebSocketServer>()?;
    m.add_class::<PyMcapWriter>()?;
    m.add_class::<PartialMetadata>()?;
    m.add_class::<Capability>()?;

    // Register the schema & channel modules
    // A declarative submodule is created in generated/schemas_module.rs, but this is currently
    // easier to work with and function modules haven't yet been deprecated.
    schemas::register_submodule(m)?;
    channels::register_submodule(m)?;
    Ok(())
}
