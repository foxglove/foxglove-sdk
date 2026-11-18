//! Private in-process codec bridge for experimental alignment conformance.
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use foxglove_mcap_alignment::executor::{
    Compression, ContextBinding, ExecuteLimits, Independence, Job, OutputOptions, SampledStream,
    TimelineConfig, execute_with_media,
};
use foxglove_mcap_alignment::local_source::{Binding, LocalSources, SourceLimits};
use foxglove_mcap_alignment::media::{
    CalibrationConfig, EncodedFrame, MediaConfig, MediaDecoder, MediaProcessor,
};
use foxglove_mcap_alignment::{Episode, Error, ErrorKind, MetadataSource, Policy, Rate};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde_json::{Value, json};

fn failure(kind: ErrorKind, message: &'static str, detail: impl ToString) -> Error {
    Error {
        kind,
        message,
        detail: Some(detail.to_string()),
        target: None,
        stream: None,
    }
}
fn py_error(py: Python<'_>, err: PyErr) -> Error {
    let kind = err
        .value(py)
        .getattr("kind")
        .and_then(|v| v.extract::<String>())
        .unwrap_or_default();
    let category = match kind.as_str() {
        "invalid_configuration" => ErrorKind::InvalidConfiguration,
        "resource_limit" => ErrorKind::ResourceLimit,
        "decode_failed" => ErrorKind::DecodeFailed,
        "encode_failed" => ErrorKind::EncodeFailed,
        _ if err.is_instance_of::<pyo3::exceptions::PyMemoryError>(py) => ErrorKind::ResourceLimit,
        _ => ErrorKind::UnsupportedMedia,
    };
    failure(category, "Python media backend", err)
}
fn to_python<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?
        .call_method1("loads", (value.to_string(),))
}
fn from_python(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    let text: String = value
        .py()
        .import("json")?
        .call_method1("dumps", (value,))?
        .extract()?;
    serde_json::from_str(&text).map_err(|e| PyValueError::new_err(e.to_string()))
}
struct PythonMedia;
struct PythonDecoder {
    decoder: Py<PyAny>,
}
impl Drop for PythonDecoder {
    fn drop(&mut self) {
        Python::attach(|py| {
            if let Err(error) = self.decoder.bind(py).call_method0("close") {
                error.write_unraisable(py, Some(self.decoder.bind(py)));
            }
        });
    }
}
fn frames(value: Bound<'_, PyAny>) -> PyResult<Vec<EncodedFrame>> {
    value
        .try_iter()?
        .map(|item| {
            let item = item?;
            Ok(EncodedFrame {
                token: item.get_item(0)?.extract()?,
                bytes: item.get_item(1)?.extract()?,
                metadata: from_python(&item.get_item(2)?)?,
            })
        })
        .collect()
}
impl MediaDecoder for PythonDecoder {
    fn feed(
        &mut self,
        data: &[u8],
        token: u64,
        selected: bool,
    ) -> Result<Vec<EncodedFrame>, Error> {
        Python::attach(|py| {
            self.decoder
                .bind(py)
                .call_method1("feed", (PyBytes::new(py, data), token, selected))
                .and_then(frames)
                .map_err(|e| py_error(py, e))
        })
    }
    fn flush(&mut self) -> Result<Vec<EncodedFrame>, Error> {
        Python::attach(|py| {
            self.decoder
                .bind(py)
                .call_method0("flush")
                .and_then(frames)
                .map_err(|e| py_error(py, e))
        })
    }
}
impl MediaProcessor for PythonMedia {
    fn image(
        &self,
        data: &[u8],
        format: &str,
        config: &MediaConfig,
    ) -> Result<EncodedFrame, Error> {
        Python::attach(|py| {
            let result = (|| -> PyResult<_> {
                let value = py.import("foxglove._alignment_media")?.call_method1(
                    "process_image",
                    (
                        PyBytes::new(py, data),
                        format,
                        to_python(py, &config.resize)?,
                        to_python(py, &config.encoding)?,
                        config.max_frame_bytes,
                    ),
                )?;
                Ok(EncodedFrame {
                    token: 0,
                    bytes: value.get_item(0)?.extract()?,
                    metadata: from_python(&value.get_item(1)?)?,
                })
            })();
            result.map_err(|e| py_error(py, e))
        })
    }
    fn capabilities(&self) -> Result<Value, Error> {
        Python::attach(|py| {
            let result = (|| {
                from_python(
                    &py.import("foxglove._alignment_media")?
                        .call_method0("capabilities")?,
                )
            })();
            result.map_err(|e| py_error(py, e))
        })
    }
    fn decoder(&self, codec: &str, config: &MediaConfig) -> Result<Box<dyn MediaDecoder>, Error> {
        Python::attach(|py| {
            let result = (|| -> PyResult<_> {
                let module = py.import("foxglove._alignment_media")?;
                let decoder = module
                    .getattr("Decoder")?
                    .call1((
                        codec,
                        to_python(py, &config.resize)?,
                        to_python(py, &config.encoding)?,
                        config.max_frame_bytes,
                        1,
                    ))?
                    .unbind();
                Ok(Box::new(PythonDecoder { decoder }) as Box<dyn MediaDecoder>)
            })();
            result.map_err(|e| py_error(py, e))
        })
    }
    fn derive_calibration(&self, calibration: &Value, geometry: &Value) -> Result<Value, Error> {
        Python::attach(|py| {
            let result = (|| {
                from_python(&py.import("foxglove._alignment_media")?.call_method1(
                    "derive_calibration",
                    (to_python(py, calibration)?, to_python(py, geometry)?),
                )?)
            })();
            result.map_err(|e| py_error(py, e))
        })
    }
}
fn invalid(message: impl ToString) -> Error {
    failure(
        ErrorKind::InvalidConfiguration,
        "private alignment configuration",
        message,
    )
}
fn integer(value: &Value, name: &str) -> Result<u64, Error> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .ok_or_else(|| invalid(name))
}
fn number(value: &Value, key: &str, default: u64) -> Result<u64, Error> {
    value.get(key).map_or(Ok(default), |v| integer(v, key))
}
fn bindings(value: &Value, sources: &LocalSources) -> Result<Vec<Binding>, Error> {
    if let Some(topic) = value.get("topic").and_then(Value::as_str) {
        return Ok(vec![sources.resolve_topic(topic)?]);
    }
    value["bindings"]
        .as_array()
        .ok_or_else(|| invalid("bindings array required"))?
        .iter()
        .map(|v| {
            Ok(Binding {
                file: usize::try_from(integer(&v["file"], "binding file")?).map_err(invalid)?,
                channel: u16::try_from(integer(&v["channel"], "binding channel")?)
                    .map_err(invalid)?,
            })
        })
        .collect()
}
fn run(value: Value) -> Result<Value, Error> {
    let paths = value["paths"]
        .as_array()
        .ok_or_else(|| invalid("paths required"))?
        .iter()
        .map(|p| {
            p.as_str()
                .map(PathBuf::from)
                .ok_or_else(|| invalid("path must be string"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let temp = PathBuf::from(value["temporary_directory"].as_str().unwrap_or("/tmp"));
    let limits = &value["limits"];
    let source_limits = SourceLimits {
        index_memory_bytes: number(limits, "index_memory_bytes", 16 * 1024 * 1024)? as usize,
        inventory_memory_bytes: number(limits, "inventory_memory_bytes", 8 * 1024 * 1024)? as usize,
        max_record_bytes: number(limits, "max_record_bytes", 8 * 1024 * 1024)? as usize,
        max_chunk_bytes: number(limits, "max_chunk_bytes", 64 * 1024 * 1024)? as usize,
        max_decompressed_chunk_bytes: number(
            limits,
            "max_decompressed_chunk_bytes",
            128 * 1024 * 1024,
        )? as usize,
        temporary_bytes: number(limits, "temporary_bytes", 68_719_476_736)?,
        max_open_files: number(limits, "max_open_files", 256)? as usize,
    };
    if let Some(mode) = value.get("index_mode")
        && mode != "indexed"
    {
        return Err(invalid("index_mode must be indexed"));
    }
    let sources = LocalSources::open(&paths, source_limits, &temp)?;
    if value["operation"] == "inspect" {
        let mut files = Vec::new();
        let mut inspection_bytes = 0usize;
        for (file, inventory) in sources.files().iter().enumerate() {
            let mut channels = Vec::new();
            for (&channel, descriptor) in &inventory.channels {
                inspection_bytes = inspection_bytes.saturating_add(
                    8192usize
                        .saturating_add(descriptor.topic.len().saturating_mul(8))
                        .saturating_add(descriptor.message_encoding.len().saturating_mul(8))
                        .saturating_add(
                            inventory
                                .schemas
                                .get(&descriptor.schema_id)
                                .map_or(0, |s| s.name.len().saturating_mul(8)),
                        ),
                );
                if inspection_bytes
                    > number(limits, "inventory_memory_bytes", 8 * 1024 * 1024)? as usize
                {
                    return Err(failure(
                        ErrorKind::ResourceLimit,
                        "inspection report memory budget",
                        inspection_bytes,
                    ));
                }
                let mut cursor = sources.metadata_source(&[Binding { file, channel }])?;
                let mut count = 0u64;
                let mut first = None;
                let mut last = None;
                let mut minimum = None;
                let mut maximum = 0u64;
                let mut spacing_histogram = [0u64; 65];
                while let Some(record) = cursor.next_record()? {
                    if let Some(previous) = last {
                        let spacing = record.log_time - previous;
                        minimum = Some(minimum.map_or(spacing, |v: u64| v.min(spacing)));
                        maximum = maximum.max(spacing);
                        let bin = (64 - spacing.leading_zeros()) as usize;
                        spacing_histogram[bin] += 1;
                    }
                    first.get_or_insert(record.log_time);
                    last = Some(record.log_time);
                    count += 1;
                }
                let schema = inventory.schemas.get(&descriptor.schema_id);
                channels.push(json!({"channel":channel,"topic":descriptor.topic,
                    "message_encoding":descriptor.message_encoding,"schema":schema.map(|s| &s.name),
                    "messages":count,"first_log_time":first.map(|v|v.to_string()),
                    "last_log_time":last.map(|v|v.to_string()),"minimum_spacing_ns":minimum.map(|v|v.to_string()),
                    "maximum_spacing_ns":if count>1 {Some(maximum.to_string())} else {None},
                    "spacing_histogram":spacing_histogram.as_slice()}));
            }
            files.push(json!({"file":file,"path":value["paths"][file],
                "bytes":inventory.identity.byte_length,"sha256":inventory.identity.sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),
                "channels":channels}));
        }
        sources.validate_sources()?;
        return Ok(
            json!({"files":files,"clock":"log_time", "spacing_histogram_bins":"0 is zero; bin k>=1 covers [2^(k-1), 2^k) ns; exact counts, approximate distribution"}),
        );
    }
    let mut streams = Vec::new();
    let mut media = Vec::new();
    for stream in value["streams"]
        .as_array()
        .ok_or_else(|| invalid("streams required"))?
    {
        streams.push(SampledStream {
            bindings: bindings(stream, &sources)?,
            output_topic: stream["output_topic"]
                .as_str()
                .ok_or_else(|| invalid("output_topic required"))?
                .into(),
            policy: match stream["policy"].as_str().unwrap_or("previous") {
                "previous" => Policy::Previous,
                "nearest" => Policy::Nearest,
                "exact" => Policy::Exact,
                _ => return Err(invalid("unknown selection policy")),
            },
            tolerance_ns: integer(&stream["tolerance_ns"], "finite tolerance_ns required")?,
            required: stream["required"].as_bool().unwrap_or(true),
            independence: match stream["independence"].as_str().unwrap_or("declared") {
                "numeric" => Independence::Numeric,
                "standalone_image" => Independence::StandaloneImage,
                _ => Independence::Declared,
            },
        });
        media.push(if let Some(config) = stream.get("media") {
            let calibration = if config["calibration"]["status"] == "unavailable" {
                CalibrationConfig::Unavailable
            } else {
                let c = &config["calibration"];
                CalibrationConfig::Bound {
                    bindings: bindings(c, &sources)?,
                    pixel_coordinates: c["pixel_coordinates"]
                        .as_str()
                        .ok_or_else(|| invalid("calibration pixel_coordinates required"))?
                        .into(),
                    original_topic: c["original_topic"]
                        .as_str()
                        .ok_or_else(|| invalid("original calibration topic required"))?
                        .into(),
                    derived_topic: c["derived_topic"]
                        .as_str()
                        .ok_or_else(|| invalid("derived calibration topic required"))?
                        .into(),
                }
            };
            Some(MediaConfig {
                resize: config
                    .get("resize")
                    .cloned()
                    .unwrap_or_else(|| json!({"kind":"none"})),
                encoding: config
                    .get("encoding")
                    .cloned()
                    .unwrap_or_else(|| json!({"kind":"png"})),
                calibration,
                max_frame_bytes: number(config, "max_frame_bytes", 64 * 1024 * 1024)? as usize,
            })
        } else {
            None
        });
    }
    let t = &value["timeline"];
    let timeline = match t["kind"].as_str().unwrap_or("fixed") {
        "fixed" => TimelineConfig::Fixed {
            rate: Rate::new(
                integer(&t["numerator"], "rate numerator")?,
                number(t, "denominator", 1)?,
            )?,
            origin: number(
                t,
                "origin",
                integer(&value["episode"]["start"], "episode start")?,
            )?,
        },
        "reference" => TimelineConfig::Reference {
            bindings: bindings(t, &sources)?,
        },
        _ => return Err(invalid("unknown timeline kind")),
    };
    let mut context = Vec::new();
    if let Some(entries) = value.get("context").and_then(Value::as_array) {
        for entry in entries {
            let file =
                usize::try_from(integer(&entry["file"], "context file")?).map_err(invalid)?;
            if file >= sources.files().len() {
                return Err(invalid("context file out of range"));
            }
            match entry["kind"].as_str() {
                Some("message") => {
                    let channel = u16::try_from(integer(&entry["channel"], "context channel")?)
                        .map_err(invalid)?;
                    let ordinal = integer(&entry["ordinal"], "context ordinal")?;
                    let mut cursor = sources.metadata_source(&[Binding { file, channel }])?;
                    let mut position = 0;
                    let source = loop {
                        let record = cursor
                            .next_record()?
                            .ok_or_else(|| invalid("context ordinal out of range"))?;
                        if position == ordinal {
                            break record;
                        }
                        position += 1;
                    };
                    context.push(ContextBinding::Message {
                        source,
                        output_topic: entry
                            .get("output_topic")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    });
                }
                Some("record") => context.push(ContextBinding::Record {
                    file,
                    offset: integer(&entry["offset"], "context record offset")?,
                    rename: entry
                        .get("rename")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }),
                _ => return Err(invalid("unknown context kind")),
            }
        }
    }
    let output = value.get("output").cloned().unwrap_or_else(|| json!({}));
    let output_object = output
        .as_object()
        .ok_or_else(|| invalid("output must be an object"))?;
    if output_object
        .keys()
        .any(|k| !["compression", "chunk_bytes", "zstd_level"].contains(&k.as_str()))
    {
        return Err(invalid("unknown output field"));
    }
    let compression = match output.get("compression").and_then(Value::as_str) {
        None if !output_object.contains_key("compression") => None,
        Some("none") => None,
        Some("lz4") => Some(Compression::Lz4),
        Some("zstd") => Some(Compression::Zstd),
        _ => return Err(invalid("invalid output compression")),
    };
    if output_object.contains_key("zstd_level") && !matches!(compression, Some(Compression::Zstd)) {
        return Err(invalid("zstd_level requires zstd compression"));
    }
    let strict_number = |object: &Value, key: &str, default: u64| -> Result<u64, Error> {
        object
            .get(key)
            .map_or(Ok(default), |v| v.as_u64().ok_or_else(|| invalid(key)))
    };
    let job = Job {
        output: OutputOptions {
            compression,
            chunk_bytes: strict_number(&output, "chunk_bytes", 1048576)?,
            zstd_level: u32::try_from(strict_number(&output, "zstd_level", 1)?).map_err(invalid)?,
        },
        episode: Episode::new(
            integer(&value["episode"]["start"], "episode start")?,
            integer(&value["episode"]["end"], "episode end")?,
        )?,
        timeline,
        streams,
        context,
        limits: ExecuteLimits {
            compute_threads: usize::try_from(strict_number(limits, "compute_threads", 7)?)
                .map_err(invalid)?,
            batch_targets: number(limits, "batch_targets", 32)? as usize,
            cache_bytes: number(limits, "cache_bytes", 64 * 1024 * 1024)? as usize,
            max_output_bytes: number(limits, "max_output_bytes", 68_719_476_736)?,
            writer_index_memory_bytes: number(
                limits,
                "writer_index_memory_bytes",
                128 * 1024 * 1024,
            )? as usize,
        },
    };
    let destination = PathBuf::from(
        value["destination"]
            .as_str()
            .ok_or_else(|| invalid("destination required"))?,
    );
    let cancellation = AtomicBool::new(false);
    let report = if media.iter().any(Option::is_some) {
        execute_with_media(
            &sources,
            job,
            &destination,
            &cancellation,
            &media,
            &PythonMedia,
        )
    } else {
        foxglove_mcap_alignment::executor::execute(&sources, job, &destination, &cancellation)
    }?;
    Ok(
        json!({"report":report.json,"output_bytes":report.output_bytes,"peak_temporary_bytes":report.peak_temporary_bytes,"finalization_ns":report.finalization_ns.to_string(),"validation_ns":report.validation_ns.to_string(),"publication_ns":report.publication_ns.to_string()}),
    )
}
/// Internal test/implementation bridge. No public Python job API is promised.
#[pyfunction]
pub(crate) fn _alignment_execute(py: Python<'_>, config_json: &str) -> PyResult<String> {
    let value: Value =
        serde_json::from_str(config_json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    py.detach(move || run(value))
        .map(|value| value.to_string())
        .map_err(|e| {
            let exception = PyValueError::new_err(e.to_string());
            let value = exception.value(py);
            let _ = value.setattr("kind", format!("{:?}", e.kind));
            let _ = value.setattr("target", e.target);
            let _ = value.setattr("stream", e.stream);
            let _ = value.setattr("detail", e.detail);
            exception
        })
}
