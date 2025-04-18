use errors::PyFoxgloveError;
use foxglove::{
    ChannelBuilder, Context, McapWriter, McapWriterHandle, PartialMetadata, RawChannel, Schema,
};
use generated::channels;
use generated::schemas;
use log::LevelFilter;
use pyo3::prelude::*;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use websocket::start_server;

mod errors;
mod generated;
mod schemas_wkt;
mod websocket;

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
    fn new(name: String, encoding: String, data: Vec<u8>) -> Self {
        Self {
            name,
            encoding,
            data,
        }
    }
}

impl From<PySchema> for foxglove::Schema {
    fn from(value: PySchema) -> Self {
        foxglove::Schema::new(value.name, value.encoding, value.data)
    }
}

#[pyclass(module = "foxglove")]
struct BaseChannel(Arc<RawChannel>);

/// A writer for logging messages to an MCAP file.
///
/// Obtain an instance by calling :py:func:`open_mcap`.
///
/// This class may be used as a context manager, in which case the writer will
/// be closed when you exit the context.
///
/// If the writer is not closed by the time it is garbage collected, it will be
/// closed automatically, and any errors will be logged.
#[pyclass(name = "MCAPWriter", module = "foxglove")]
struct PyMcapWriter(Option<McapWriterHandle<BufWriter<File>>>);

impl Drop for PyMcapWriter {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("Failed to close MCAP writer: {e}");
        }
    }
}

#[pymethods]
impl PyMcapWriter {
    fn __enter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: Py<PyAny>,
        _exc_value: Py<PyAny>,
        _traceback: Py<PyAny>,
    ) -> PyResult<()> {
        self.close()
    }

    /// Close the MCAP writer.
    ///
    /// You may call this to explicitly close the writer. Note that the writer will be automatically
    /// closed for you when it is garbage collected, or when exiting the context manager.
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
        signature = (topic, message_encoding, schema=None, metadata=None)
    )]
    fn new(
        topic: &str,
        message_encoding: &str,
        schema: Option<PySchema>,
        metadata: Option<BTreeMap<String, String>>,
    ) -> PyResult<Self> {
        let channel = ChannelBuilder::new(topic)
            .message_encoding(message_encoding)
            .schema(schema.map(Schema::from))
            .metadata(metadata.unwrap_or_default())
            .build_raw()
            .map_err(PyFoxgloveError::from)?;

        Ok(BaseChannel(channel))
    }

    #[pyo3(signature = (msg, log_time=None))]
    fn log(&self, msg: &[u8], log_time: Option<u64>) -> PyResult<()> {
        let metadata = PartialMetadata { log_time };
        self.0.log_with_meta(msg, metadata);
        Ok(())
    }

    fn id(&self) -> u64 {
        self.0.id().into()
    }

    fn topic(&self) -> &str {
        self.0.topic()
    }

    fn schema_name(&self) -> Option<&str> {
        Some(self.0.schema()?.name.as_str())
    }

    fn close(&mut self) {
        self.0.close();
    }
}

/// Open a new mcap file for recording.
///
/// :param path: The path to the MCAP file. This file will be created and must not already exist.
/// :param allow_overwrite: Set this flag in order to overwrite an existing file at this path.
/// :rtype: :py:class:`MCAPWriter`
#[pyfunction]
#[pyo3(signature = (path, *, allow_overwrite = false))]
fn open_mcap(path: PathBuf, allow_overwrite: bool) -> PyResult<PyMcapWriter> {
    let file = if allow_overwrite {
        File::create(path)?
    } else {
        File::create_new(path)?
    };
    let writer = BufWriter::new(file);
    let handle = McapWriter::new()
        .create(writer)
        .map_err(PyFoxgloveError::from)?;
    Ok(PyMcapWriter(Some(handle)))
}

#[pyfunction]
fn get_channel_for_topic(topic: &str) -> PyResult<Option<BaseChannel>> {
    let channel = Context::get_default().get_channel_by_topic(topic);
    Ok(channel.map(BaseChannel))
}

// Not public. Re-exported in a wrapping function.
#[pyfunction]
fn enable_logging(level: u32) -> PyResult<()> {
    // SDK will not log at levels "CRITICAL" or higher.
    // https://docs.python.org/3/library/logging.html#logging-levels
    let level = match level {
        50.. => LevelFilter::Off,
        40.. => LevelFilter::Error,
        30.. => LevelFilter::Warn,
        20.. => LevelFilter::Info,
        10.. => LevelFilter::Debug,
        0.. => LevelFilter::Trace,
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
    foxglove::library_version::set_sdk_language("python");
    pyo3_log::init();
    m.add_function(wrap_pyfunction!(enable_logging, m)?)?;
    m.add_function(wrap_pyfunction!(disable_logging, m)?)?;
    m.add_function(wrap_pyfunction!(shutdown, m)?)?;
    m.add_function(wrap_pyfunction!(open_mcap, m)?)?;
    m.add_function(wrap_pyfunction!(start_server, m)?)?;
    m.add_function(wrap_pyfunction!(get_channel_for_topic, m)?)?;
    m.add_class::<BaseChannel>()?;
    m.add_class::<PyMcapWriter>()?;
    m.add_class::<PySchema>()?;

    // Register nested modules.
    schemas::register_submodule(m)?;
    channels::register_submodule(m)?;
    websocket::register_submodule(m)?;
    Ok(())
}
