use errors::PyFoxgloveError;
use foxglove::McapWriteOptions;
use foxglove::{ChannelBuilder, Context, McapWriter, PartialMetadata, RawChannel, Schema};
use generated::channels;
use generated::schemas;
use log::LevelFilter;
use mcap::{PyMcapWriteOptions, PyMcapWriter};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use websocket::start_server;

mod errors;
mod generated;
mod mcap;
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
#[pyclass(name = "Schema", module = "foxglove", get_all, set_all, eq)]
#[derive(Clone, PartialEq, Eq)]
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

impl From<foxglove::Schema> for PySchema {
    fn from(value: foxglove::Schema) -> Self {
        Self::new(value.name, value.encoding, value.data.into_owned())
    }
}

#[pyclass(module = "foxglove")]
struct BaseChannel(Arc<RawChannel>);

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

    #[getter]
    fn message_encoding(&self) -> &str {
        self.0.message_encoding()
    }

    fn metadata(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        for (key, value) in self.0.metadata() {
            dict.set_item(key, value)?;
        }
        Ok(dict.into())
    }

    fn schema(&self) -> Option<PySchema> {
        self.0.schema().cloned().map(PySchema::from)
    }

    fn schema_name(&self) -> Option<&str> {
        Some(self.0.schema()?.name.as_str())
    }

    fn has_sinks(&self) -> bool {
        self.0.has_sinks()
    }

    fn close(&mut self) {
        self.0.close();
    }
}

/// A context for logging messages.
///
/// A context is the binding between channels and sinks. By default, the SDK will use a single
/// global context for logging, but you can create multiple contexts in order to log to different
/// topics to different sinks or servers. To do so, associate the context by passing it to the
/// channel constructor and to :py:func:`open_mcap` or :py:func:`start_server`.
#[pyclass(module = "foxglove", name = "Context")]
struct PyContext(pub(crate) Arc<foxglove::Context>);

#[pymethods]
impl PyContext {
    #[new]
    fn new() -> Self {
        Self(foxglove::Context::new())
    }

    #[staticmethod]
    fn default() -> Self {
        Self(foxglove::Context::get_default())
    }

    /// Create a new channel for logging messages on a topic.
    ///
    /// Python users should pass a Context to a channel constructor instead of calling this method
    /// directly.
    #[pyo3(signature = (topic, *, message_encoding, schema=None, metadata=None))]
    fn _create_channel(
        &self,
        topic: &str,
        message_encoding: &str,
        schema: Option<PySchema>,
        metadata: Option<BTreeMap<String, String>>,
    ) -> PyResult<BaseChannel> {
        let channel = self
            .0
            .channel_builder(topic)
            .message_encoding(message_encoding)
            .schema(schema.map(Schema::from))
            .metadata(metadata.unwrap_or_default())
            .build_raw()
            .map_err(PyFoxgloveError::from)?;
        Ok(BaseChannel(channel))
    }
}

/// Open a new mcap file for recording.
///
/// :param path: The path to the MCAP file. This file will be created and must not already exist.
/// :type path: str | Path
/// :param allow_overwrite: Set this flag in order to overwrite an existing file at this path.
/// :type allow_overwrite: Optional[bool]
/// :param context: The context to use for logging. If None, the global context is used.
/// :type context: :py:class:`Context`
/// :param writer_options: Options for the MCAP writer.
/// :type writer_options: :py:class:`mcap.MCAPWriteOptions`
/// :rtype: :py:class:`mcap.MCAPWriter`
#[pyfunction]
#[pyo3(signature = (path, *, allow_overwrite = false, context = None, writer_options = None))]
fn open_mcap(
    path: PathBuf,
    allow_overwrite: bool,
    context: Option<PyRef<PyContext>>,
    writer_options: Option<PyMcapWriteOptions>,
) -> PyResult<PyMcapWriter> {
    let file = if allow_overwrite {
        File::create(path)?
    } else {
        File::create_new(path)?
    };

    let options = writer_options.map_or_else(McapWriteOptions::default, |opts| opts.into());
    let writer = BufWriter::new(file);
    let handle = if let Some(context) = context {
        McapWriter::with_options(options).context(&context.0)
    } else {
        McapWriter::with_options(options)
    };
    let handle = handle.create(writer).map_err(PyFoxgloveError::from)?;
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
    m.add_class::<PySchema>()?;
    m.add_class::<PyContext>()?;
    // Register nested modules.
    schemas::register_submodule(m)?;
    channels::register_submodule(m)?;
    mcap::register_submodule(m)?;
    websocket::register_submodule(m)?;
    Ok(())
}
