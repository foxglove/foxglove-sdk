use errors::PyFoxgloveError;
use foxglove::{
    Channel, ChannelBuilder, LogContext, McapWriter, McapWriterHandle, PartialMetadata, Schema,
    WebSocketServer, WebSocketServerBlockingHandle,
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
mod py_schemas;

#[pyclass]
struct BaseChannel(Arc<Channel>);

#[pyclass]
struct PyWebSocketServer(Option<WebSocketServerBlockingHandle>);

#[pymethods]
impl PyWebSocketServer {
    fn stop(&mut self, py: Python<'_>) {
        if let Some(server) = self.0.take() {
            py.allow_threads(|| server.stop())
        }
    }
}

#[pyclass]
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
        let metadata = PartialMetadata {
            sequence,
            log_time,
            publish_time,
        };
        self.0.log_with_meta(msg, metadata);
        Ok(())
    }
}

/// Open a new mcap file for recording
#[pyfunction]
#[pyo3(signature = (path))]
fn record_file(path: &str) -> PyResult<PyMcapWriter> {
    let handle = McapWriter::new()
        .create_new_buffered_file(path)
        .map_err(PyFoxgloveError::from)?;
    Ok(PyMcapWriter(Some(handle)))
}

/// Start a new Foxglove WebSocket server
#[pyfunction]
#[pyo3(signature = (name = None, host="127.0.0.1", port=0))]
fn start_server(
    py: Python<'_>,
    name: Option<String>,
    host: &str,
    port: u16,
) -> PyResult<PyWebSocketServer> {
    let session_id = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .expect("Failed to create session ID; invalid system time")
        .as_millis()
        .to_string();

    let mut server = WebSocketServer::new()
        .session_id(session_id)
        .bind(host, port);
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

#[pyfunction]
fn enable_log_forwarding(level: &str) -> PyResult<()> {
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

#[pyfunction]
fn disable_log_forwarding() -> PyResult<()> {
    log::set_max_level(LevelFilter::Off);
    Ok(())
}

#[pyfunction]
fn shutdown(py: Python<'_>) {
    py.allow_threads(foxglove::shutdown_runtime);
}

/// Our public API is in the `python` directory.
/// Rust bindings are exported as `_foxglove_py` and should not be imported directly.
#[pymodule]
mod _foxglove_py {
    use super::*;

    #[pymodule_export]
    use super::disable_log_forwarding;
    #[pymodule_export]
    use super::enable_log_forwarding;
    #[pymodule_export]
    use super::get_channel_for_topic;
    #[pymodule_export]
    use super::record_file;
    #[pymodule_export]
    use super::shutdown;
    #[pymodule_export]
    use super::start_server;

    // include!("./schmodule.rs");
    #[pymodule]
    mod schemas {
        use pyo3::types::PyAnyMethods;
        use pyo3::types::PyModule;
        use pyo3::Bound;
        use pyo3::PyResult;

        #[pymodule_export]
        use super::py_schemas::LinePrimitiveLineType;

        #[pymodule_export]
        use super::py_schemas::LogLevel;

        #[pymodule_export]
        use super::py_schemas::SceneEntityDeletionType;

        #[pymodule_export]
        use super::py_schemas::PackedElementFieldNumericType;

        #[pymodule_export]
        use super::py_schemas::PointsAnnotationType;

        #[pymodule_export]
        use super::py_schemas::LocationFixPositionCovarianceType;

        #[pymodule_export]
        use super::py_schemas::ArrowPrimitive;

        #[pymodule_export]
        use super::py_schemas::CameraCalibration;

        #[pymodule_export]
        use super::py_schemas::CircleAnnotation;

        #[pymodule_export]
        use super::py_schemas::Color;

        #[pymodule_export]
        use super::py_schemas::CompressedImage;

        #[pymodule_export]
        use super::py_schemas::CompressedVideo;

        #[pymodule_export]
        use super::py_schemas::CylinderPrimitive;

        #[pymodule_export]
        use super::py_schemas::CubePrimitive;

        #[pymodule_export]
        use super::py_schemas::FrameTransform;

        #[pymodule_export]
        use super::py_schemas::FrameTransforms;

        #[pymodule_export]
        use super::py_schemas::GeoJson;

        #[pymodule_export]
        use super::py_schemas::Grid;

        #[pymodule_export]
        use super::py_schemas::ImageAnnotations;

        #[pymodule_export]
        use super::py_schemas::KeyValuePair;

        #[pymodule_export]
        use super::py_schemas::LaserScan;

        #[pymodule_export]
        use super::py_schemas::LinePrimitive;

        #[pymodule_export]
        use super::py_schemas::LocationFix;

        #[pymodule_export]
        use super::py_schemas::Log;

        #[pymodule_export]
        use super::py_schemas::SceneEntityDeletion;

        #[pymodule_export]
        use super::py_schemas::SceneEntity;

        #[pymodule_export]
        use super::py_schemas::SceneUpdate;

        #[pymodule_export]
        use super::py_schemas::ModelPrimitive;

        #[pymodule_export]
        use super::py_schemas::PackedElementField;

        #[pymodule_export]
        use super::py_schemas::Point2;

        #[pymodule_export]
        use super::py_schemas::Point3;

        #[pymodule_export]
        use super::py_schemas::PointCloud;

        #[pymodule_export]
        use super::py_schemas::PointsAnnotation;

        #[pymodule_export]
        use super::py_schemas::Pose;

        #[pymodule_export]
        use super::py_schemas::PoseInFrame;

        #[pymodule_export]
        use super::py_schemas::PosesInFrame;

        #[pymodule_export]
        use super::py_schemas::Quaternion;

        #[pymodule_export]
        use super::py_schemas::RawImage;

        #[pymodule_export]
        use super::py_schemas::SpherePrimitive;

        #[pymodule_export]
        use super::py_schemas::TextAnnotation;

        #[pymodule_export]
        use super::py_schemas::TextPrimitive;

        #[pymodule_export]
        use super::py_schemas::TriangleListPrimitive;

        #[pymodule_export]
        use super::py_schemas::Vector2;

        #[pymodule_export]
        use super::py_schemas::Vector3;

        #[pymodule_init]
        fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
            // https://github.com/PyO3/pyo3/issues/759
            let py = m.py();
            py.import("sys")?
                .getattr("modules")?
                .set_item("foxglove._foxglove_py.schemas", m)
        }
    }

    #[pymodule_export]
    use super::BaseChannel;
    #[pymodule_export]
    use super::PyMcapWriter;
    #[pymodule_export]
    use super::PyWebSocketServer;
}
