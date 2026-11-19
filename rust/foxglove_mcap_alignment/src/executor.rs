//! Experimental bounded independent-message materialization.
//!
//! The atomic artifact is one MCAP containing provenance, run configuration and report.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use mcap::records::{MessageHeader, Metadata, Record};
use prost::Message;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::local_source::payload::PayloadReader;
use crate::local_source::{Binding, LocalSources, error};
use crate::{
    Episode, Error, ErrorKind, Locator, Policy, Rate, Selector, SourceRecord, Stream, Timeline,
};

pub use mcap::Compression;

pub const PROVENANCE_TOPIC: &str = "/foxglove/alignment/provenance/v1";
pub const REPORT_METADATA: &str = "foxglove.alignment.report.v1";
pub const RUN_METADATA: &str = "foxglove.alignment.run.v1";
/// Frozen wire contract, with decimal strings for every nanosecond value.
pub const PROVENANCE_SCHEMA: &str = include_str!("provenance-v1.schema.json");

/// Classification is caller supplied; known inter-frame video always fails.
#[derive(Clone, Copy, Debug)]
pub enum Independence {
    Declared,
    Numeric,
    StandaloneImage,
}

pub struct SampledStream {
    pub bindings: Vec<Binding>,
    pub output_topic: String,
    pub policy: Policy,
    pub tolerance_ns: u64,
    pub required: bool,
    pub independence: Independence,
}
pub enum TimelineConfig {
    Fixed { rate: Rate, origin: u64 },
    Reference { bindings: Vec<Binding> },
}
/// Explicit persistent context. Message references must match the validated source index.
pub enum ContextBinding {
    Message {
        source: SourceRecord,
        output_topic: Option<String>,
    },
    Record {
        file: usize,
        offset: u64,
        rename: Option<String>,
    },
}
/// Explicit memory reservation for retained writer indexes, schemas and channel maps.
/// The cache keeps one chunk; chunks above cache capacity use bounded fallback reads.
pub struct ExecuteLimits {
    /// Total compute budget, including coordinator, camera workers and codec allowances.
    pub compute_threads: usize,
    pub batch_targets: usize,
    pub cache_bytes: usize,
    pub max_output_bytes: u64,
    pub writer_index_memory_bytes: usize,
}
/// Output layout controls for development sweeps; existing defaults are unchanged.
#[derive(Clone, Copy, Debug)]
pub struct OutputOptions {
    pub compression: Option<mcap::Compression>,
    pub chunk_bytes: u64,
    pub zstd_level: u32,
}
impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            compression: None,
            chunk_bytes: 1048576,
            zstd_level: 1,
        }
    }
}
impl OutputOptions {
    fn validate(self) -> Result<(), Error> {
        if ![262144, 1048576, 8388608].contains(&self.chunk_bytes)
            || !(1..=3).contains(&self.zstd_level)
            || (!matches!(self.compression, Some(mcap::Compression::Zstd)) && self.zstd_level != 1)
        {
            return Err(Error::new(
                ErrorKind::InvalidConfiguration,
                "invalid output chunk size or compression level",
            ));
        }
        Ok(())
    }
    /// MCAP writes directly to a seekable file. Reserve two chunk buffers plus
    /// 32 MiB for single-thread Zstd levels 1..3 or LZ4 streaming workspaces.
    pub(crate) fn workspace_bytes(self) -> u128 {
        self.chunk_bytes as u128 * 2
            + if self.compression.is_some() {
                32 * 1024 * 1024
            } else {
                0
            }
    }
}
/// Conservative allocation retains the existing three-camera maximum. A codec
/// allowance covers backend-internal work even when the codec runs synchronously.
pub(crate) fn media_workers(budget: usize) -> usize {
    ((budget.saturating_sub(1)) / 2).clamp(1, 3)
}
pub struct Job {
    pub output: OutputOptions,
    pub episode: Episode,
    pub timeline: TimelineConfig,
    pub streams: Vec<SampledStream>,
    pub context: Vec<ContextBinding>,
    pub limits: ExecuteLimits,
}
/// The embedded report describes work before finalization; final artifact length is
/// returned separately because the embedded report itself contributes bytes.
pub struct Report {
    pub json: Value,
    pub output_bytes: u64,
    pub peak_temporary_bytes: u64,
    pub finalization_ns: u128,
    pub validation_ns: u128,
    pub publication_ns: u128,
}
/// Constant-size signed offset summary. Each side's sum fits u128 because both
/// the number of accepted messages and each absolute offset are bounded by u64.
#[derive(Clone)]
struct SkewSummary {
    accepted: u64,
    reused: u64,
    min: Option<i128>,
    max: Option<i128>,
    positive_sum: u128,
    negative_sum: u128,
    histogram: [u64; 129],
}
impl Default for SkewSummary {
    fn default() -> Self {
        Self {
            accepted: 0,
            reused: 0,
            min: None,
            max: None,
            positive_sum: 0,
            negative_sum: 0,
            histogram: [0; 129],
        }
    }
}
impl SkewSummary {
    fn observe(&mut self, offset: i128, reused: bool) {
        self.accepted += 1;
        self.reused += u64::from(reused);
        self.min = Some(self.min.map_or(offset, |value| value.min(offset)));
        self.max = Some(self.max.map_or(offset, |value| value.max(offset)));
        let magnitude = offset.unsigned_abs();
        let bin = if magnitude == 0 {
            64
        } else {
            let exponent = magnitude.ilog2() as usize;
            if offset < 0 {
                63 - exponent
            } else {
                65 + exponent
            }
        };
        self.histogram[bin] += 1;
        if offset < 0 {
            self.negative_sum += magnitude;
        } else {
            self.positive_sum += magnitude;
        }
    }
    fn json(&self, stream: usize) -> Value {
        let numerator = if self.negative_sum > self.positive_sum {
            format!("-{}", self.negative_sum - self.positive_sum)
        } else {
            (self.positive_sum - self.negative_sum).to_string()
        };
        json!({"stream":stream,"accepted":self.accepted,"reused":self.reused,
            "skew_ns":{"min":self.min.map(|v|v.to_string()),"max":self.max.map(|v|v.to_string()),
            "mean_numerator":numerator,"mean_denominator":self.accepted.to_string(),
            "histogram":self.histogram.as_slice()}})
    }
}

fn failure(kind: ErrorKind, message: &'static str, detail: impl ToString) -> Error {
    error(kind, message, detail)
}
fn write_error(err: impl std::error::Error + 'static) -> Error {
    let mut cause: &(dyn std::error::Error + 'static) = &err;
    loop {
        if let Some(error) = cause.downcast_ref::<Error>() {
            return error.clone();
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if let Some(error) = io.get_ref().and_then(|inner| inner.downcast_ref::<Error>()) {
                return error.clone();
            }
            if matches!(io.raw_os_error(), Some(23 | 24 | 28)) {
                return failure(
                    ErrorKind::ResourceLimit,
                    "output I/O resource exhausted",
                    &err,
                );
            }
        }
        match cause.source() {
            Some(next) => cause = next,
            None => break,
        }
    }
    failure(ErrorKind::IoFailed, "MCAP output write/finalization", err)
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), Error> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::new(ErrorKind::Cancelled, "alignment job cancelled"))
    } else {
        Ok(())
    }
}
fn binding_json(bindings: &[Binding]) -> Value {
    Value::Array(
        bindings
            .iter()
            .map(|b| json!({"file":b.file,"channel":b.channel}))
            .collect(),
    )
}
pub(crate) fn locator_json(locator: Locator) -> Value {
    match locator {
        Locator::Unchunked { offset } => json!({"offset":offset.to_string()}),
        Locator::Chunked {
            chunk_offset,
            message_offset,
        } => {
            json!({"chunk_offset":chunk_offset.to_string(),"message_offset":message_offset.to_string()})
        }
    }
}
fn record_json(source: SourceRecord) -> Value {
    json!({"file":source.identity.file,"channel":source.identity.channel,"locator":locator_json(source.identity.locator),"log_time":source.log_time.to_string(),"publish_time":source.publish_time.to_string(),"sequence":source.sequence})
}
fn config_bound(job: &Job) -> Result<(), Error> {
    let mut bytes = 4096usize;
    let mut charge = |count: usize| -> Result<(), Error> {
        bytes = bytes
            .checked_add(count)
            .ok_or_else(|| Error::new(ErrorKind::ResourceLimit, "configuration size overflow"))?;
        if bytes > 512 * 1024 {
            Err(Error::new(
                ErrorKind::ResourceLimit,
                "configuration exceeds 512 KiB bounded representation",
            ))
        } else {
            Ok(())
        }
    };
    // Six bytes per UTF-8 byte bounds JSON escaping of control characters.
    for stream in &job.streams {
        charge(512)?;
        charge(stream.output_topic.len().saturating_mul(6))?;
        charge(stream.bindings.len().saturating_mul(128))?;
    }
    if let TimelineConfig::Reference { bindings } = &job.timeline {
        charge(bindings.len().saturating_mul(128))?;
    }
    for context in &job.context {
        charge(512)?;
        let name = match context {
            ContextBinding::Message { output_topic, .. } => output_topic,
            ContextBinding::Record { rename, .. } => rename,
        };
        charge(name.as_ref().map_or(0, |name| name.len().saturating_mul(6)))?;
    }
    Ok(())
}
fn resolved_config(job: &Job, sources: &LocalSources) -> Value {
    let timeline = match &job.timeline {
        TimelineConfig::Fixed { rate, origin } => {
            json!({"kind":"fixed","numerator":rate.numerator(),"denominator":rate.denominator(),"origin":origin.to_string()})
        }
        TimelineConfig::Reference { bindings } => {
            json!({"kind":"reference","bindings":binding_json(bindings)})
        }
    };
    let streams:Vec<_>=job.streams.iter().map(|s|json!({"bindings":binding_json(&s.bindings),"output_topic":s.output_topic,"policy":format!("{:?}",s.policy),"tolerance_ns":s.tolerance_ns.to_string(),"required":s.required,"independence":format!("{:?}",s.independence)})).collect();
    let context: Vec<_> = job
        .context
        .iter()
        .map(|c| match c {
            ContextBinding::Message {
                source,
                output_topic,
            } => {
                json!({"kind":"message","source":record_json(*source),"output_topic":output_topic})
            }
            ContextBinding::Record {
                file,
                offset,
                rename,
            } => json!({"kind":"record","file":file,"offset":offset.to_string(),"rename":rename}),
        })
        .collect();
    let source_limits = &sources.limits;
    json!({"index_mode":"Indexed","source_limits":{"index_memory_bytes":source_limits.index_memory_bytes,"inventory_memory_bytes":source_limits.inventory_memory_bytes,"max_record_bytes":source_limits.max_record_bytes,"max_chunk_bytes":source_limits.max_chunk_bytes,"max_decompressed_chunk_bytes":source_limits.max_decompressed_chunk_bytes,"temporary_bytes":source_limits.temporary_bytes,"max_open_files":source_limits.max_open_files},"episode":{"start":job.episode.start().to_string(),"end":job.episode.end().to_string()},"timeline":timeline,"streams":streams,"context":context,"limits":{"compute_threads":job.limits.compute_threads,"batch_targets":job.limits.batch_targets,"cache_bytes":job.limits.cache_bytes,"max_output_bytes":job.limits.max_output_bytes,"writer_index_memory_bytes":job.limits.writer_index_memory_bytes},"output":{"compute_threads":job.limits.compute_threads,"compression":match job.output.compression {None=>"none",Some(mcap::Compression::Lz4)=>"lz4",Some(mcap::Compression::Zstd)=>"zstd"},"chunk_bytes":job.output.chunk_bytes,"zstd_level":if matches!(job.output.compression, Some(mcap::Compression::Zstd)) {Some(job.output.zstd_level)} else {None},"compression_worker_threads":0,"writer_workspace_bytes":job.output.workspace_bytes(),"report":"embedded_metadata_v1"}})
}
fn metadata<W: Write + Seek>(
    writer: &mut mcap::Writer<W>,
    name: &str,
    value: &Value,
) -> Result<(), Error> {
    writer
        .write_metadata(&Metadata {
            name: name.into(),
            metadata: BTreeMap::from([("json".into(), value.to_string())]),
        })
        .map_err(write_error)
}
fn channel<W: Write + Seek>(
    sources: &LocalSources,
    writer: &mut mcap::Writer<W>,
    binding: Binding,
    topic: &str,
) -> Result<u16, Error> {
    let inventory = sources.files().get(binding.file).ok_or_else(|| {
        failure(
            ErrorKind::InvalidConfiguration,
            "unknown output binding file",
            binding.file,
        )
    })?;
    let descriptor = inventory.channels.get(&binding.channel).ok_or_else(|| {
        failure(
            ErrorKind::InvalidConfiguration,
            "unknown output channel",
            binding.channel,
        )
    })?;
    let schema = if let Some(schema) = inventory.schemas.get(&descriptor.schema_id) {
        writer
            .add_schema(&schema.name, &schema.encoding, &schema.data)
            .map_err(write_error)?
    } else {
        0
    };
    writer
        .add_channel(
            schema,
            topic,
            &descriptor.message_encoding,
            &descriptor.metadata,
        )
        .map_err(write_error)
}
fn media_channel<W: Write + Seek, T: foxglove::Encode>(
    writer: &mut mcap::Writer<W>,
    topic: &str,
    metadata: &BTreeMap<String, String>,
) -> Result<u16, Error> {
    let schema = T::get_schema().expect("canonical generated media schema");
    let id = writer
        .add_schema(&schema.name, &schema.encoding, &schema.data)
        .map_err(write_error)?;
    writer
        .add_channel(id, topic, "protobuf", metadata)
        .map_err(write_error)
}
fn interframe_video(
    schema: &str,
    serialization: &str,
    metadata: &BTreeMap<String, String>,
) -> bool {
    let schema = schema.trim().to_ascii_lowercase();
    if [
        "foxglove.compressedvideo",
        "foxglove.compressed_video",
        "foxglove_msgs/compressedvideo",
        "foxglove_msgs/msg/compressedvideo",
    ]
    .contains(&schema.as_str())
    {
        return true;
    }
    let codec = |value: &str| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "h264"
                | "h.264"
                | "avc"
                | "h265"
                | "h.265"
                | "hevc"
                | "h266"
                | "h.266"
                | "vvc"
                | "av1"
                | "vp8"
                | "vp9"
                | "video/h264"
                | "video/h.264"
                | "video/avc"
                | "video/h265"
                | "video/h.265"
                | "video/hevc"
                | "video/h266"
                | "video/h.266"
                | "video/vvc"
                | "video/av1"
                | "video/vp8"
                | "video/vp9"
        )
    };
    codec(serialization)
        || metadata.iter().any(|(key, value)| {
            matches!(
                key.trim().to_ascii_lowercase().as_str(),
                "codec"
                    | "video_codec"
                    | "media_format"
                    | "format"
                    | "encoding"
                    | "foxglove.codec"
                    | "foxglove.media_format"
            ) && codec(value)
        })
}
fn validate_independence(sources: &LocalSources, stream: &SampledStream) -> Result<(), Error> {
    for binding in &stream.bindings {
        let file = &sources.files()[binding.file];
        let channel = &file.channels[&binding.channel];
        let schema = file
            .schemas
            .get(&channel.schema_id)
            .map_or("", |s| s.name.as_str());
        if interframe_video(schema, &channel.message_encoding, &channel.metadata) {
            return Err(failure(
                ErrorKind::UnsupportedMedia,
                "inter-frame video requires media materialization",
                format!(
                    "file {}, channel {}, topic {}, schema {}",
                    binding.file, binding.channel, channel.topic, schema
                ),
            ));
        }
        match stream.independence {
            Independence::Declared => {}
            Independence::Numeric
                if schema.starts_with("foxglove.")
                    && [
                        "Vector",
                        "Pose",
                        "Quaternion",
                        "LocationFix",
                        "Imu",
                        "JointState",
                        "Float",
                        "Integer",
                        "Numeric",
                    ]
                    .iter()
                    .any(|name| schema.contains(name)) => {}
            Independence::StandaloneImage
                if [
                    "foxglove.RawImage",
                    "foxglove.CompressedImage",
                    "sensor_msgs/msg/Image",
                    "sensor_msgs/msg/CompressedImage",
                    "sensor_msgs/Image",
                    "sensor_msgs/CompressedImage",
                ]
                .contains(&schema) => {}
            _ => {
                return Err(failure(
                    ErrorKind::UnsupportedMedia,
                    "unknown opaque source requires explicit independence declaration",
                    format!(
                        "file {}, channel {}, schema {schema}",
                        binding.file, binding.channel
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Execute synchronously on one compute thread. Final publication is a no-clobber
/// hard link of a validated, synced staging file in the destination parent.
pub fn execute(
    sources: &LocalSources,
    job: Job,
    destination: &Path,
    cancellation: &AtomicBool,
) -> Result<Report, Error> {
    execute_inner(sources, job, destination, cancellation, &[], None)
}

/// Materialize configured camera streams through an internal codec boundary.
/// Independent streams share the same writer and atomic publication path.
pub fn execute_with_media(
    sources: &LocalSources,
    job: Job,
    destination: &Path,
    cancellation: &AtomicBool,
    media: &[Option<crate::media::MediaConfig>],
    backend: &dyn crate::media::MediaProcessor,
) -> Result<Report, Error> {
    if media.len() != job.streams.len() {
        return Err(Error::new(
            ErrorKind::InvalidConfiguration,
            "media configuration must match sampled stream count",
        ));
    }
    execute_inner(
        sources,
        job,
        destination,
        cancellation,
        media,
        Some(backend),
    )
}

fn execute_inner(
    sources: &LocalSources,
    job: Job,
    destination: &Path,
    cancellation: &AtomicBool,
    media: &[Option<crate::media::MediaConfig>],
    backend: Option<&dyn crate::media::MediaProcessor>,
) -> Result<Report, Error> {
    let _exclusive = sources.execution_lock()?;
    let latch = sources.write_failure_latch();
    *latch
        .lock()
        .map_err(|_| Error::new(ErrorKind::ValidationFailed, "write failure latch poisoned"))? =
        None;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_body(sources, job, destination, cancellation, media, backend)
    }));
    let failure = latch
        .lock()
        .map_err(|_| Error::new(ErrorKind::ValidationFailed, "write failure latch poisoned"))?
        .take();
    match result {
        Ok(Err(error))
            if matches!(
                error.kind,
                ErrorKind::IoFailed | ErrorKind::ValidationFailed
            ) =>
        {
            Err(failure.unwrap_or(error))
        }
        Ok(Err(error)) => Err(error),
        Ok(Ok(report)) => failure.map_or(Ok(report), Err),
        Err(panic) => Err(failure.unwrap_or_else(|| panic_error(panic))),
    }
}
fn panic_error(panic: Box<dyn std::any::Any + Send>) -> Error {
    let detail = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic");
    failure(ErrorKind::ValidationFailed, "MCAP executor unwound", detail)
}

/// Keep the upstream writer destructor inside its own unwind boundary. MCAP 0.25
/// can leave its private writer slot empty after a failed chunk initialization;
/// its destructor then panics. Catching that destructor separately also prevents
/// a second panic from escaping while another operation is already unwinding.
struct GuardedWriter<W: Write + Seek> {
    inner: Option<mcap::Writer<W>>,
    failure: std::sync::Arc<std::sync::Mutex<Option<Error>>>,
}
impl<W: Write + Seek> std::ops::Deref for GuardedWriter<W> {
    type Target = mcap::Writer<W>;
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().expect("owned writer")
    }
}
impl<W: Write + Seek> std::ops::DerefMut for GuardedWriter<W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().expect("owned writer")
    }
}
impl<W: Write + Seek> GuardedWriter<W> {
    fn into_inner(mut self) -> W {
        self.inner.take().expect("owned writer").into_inner()
    }
}
impl<W: Write + Seek> Drop for GuardedWriter<W> {
    fn drop(&mut self) {
        if let Some(writer) = self.inner.take()
            && let Err(panic) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(writer)))
            && let Ok(mut latch) = self.failure.lock()
        {
            latch.get_or_insert_with(|| panic_error(panic));
        }
    }
}
fn execute_body(
    sources: &LocalSources,
    job: Job,
    destination: &Path,
    cancellation: &AtomicBool,
    media: &[Option<crate::media::MediaConfig>],
    backend: Option<&dyn crate::media::MediaProcessor>,
) -> Result<Report, Error> {
    let _scratch = sources.scratch_guard()?;
    let _handles = sources.execution_handles()?;
    let started = Instant::now();
    check_cancel(cancellation)?;
    if destination.exists() {
        return Err(failure(
            ErrorKind::IoFailed,
            "destination already exists",
            destination.display(),
        ));
    }
    if job.streams.is_empty() || job.limits.batch_targets == 0 || job.limits.max_output_bytes == 0 {
        return Err(Error::new(
            ErrorKind::InvalidConfiguration,
            "positive batch/output limits and sampled streams required",
        ));
    }
    job.output.validate()?;
    if !(1..=8).contains(&job.limits.compute_threads) {
        return Err(Error::new(
            ErrorKind::InvalidConfiguration,
            "compute_threads must be in 1..=8",
        ));
    }
    // Payload bytes go to disk; retained writer metadata has a separate enforced budget.
    let batch_slots = job
        .limits
        .batch_targets
        .checked_mul(job.streams.len())
        .ok_or_else(|| Error::new(ErrorKind::ResourceLimit, "batch slot overflow"))?;
    let reservation = u128::from(sources.managed_bytes as u64)
        + job.limits.writer_index_memory_bytes as u128
        + job.limits.cache_bytes as u128
        + batch_slots as u128 * 512
        + job.streams.len() as u128 * 32768
        + 128 * 1024 * 1024
        + job.output.workspace_bytes()
        + sources.limits.max_decompressed_chunk_bytes as u128 * 3;
    if reservation > 4_294_967_296 {
        return Err(failure(
            ErrorKind::ResourceLimit,
            "executor and source reservations exceed shared 4 GiB managed pool",
            reservation,
        ));
    }
    config_bound(&job)?;
    let mut config = resolved_config(&job, sources);
    if !media.is_empty() {
        config["media"] = Value::Array(
            media
                .iter()
                .map(|m| m.as_ref().map_or(Value::Null, |m| m.json()))
                .collect(),
        );
    }
    let has_media = media.iter().any(Option::is_some);
    let media_tasks: BTreeSet<_> = media
        .iter()
        .enumerate()
        .filter_map(|(index, config)| config.as_ref().map(|config| (index, config)))
        .flat_map(|(index, config)| {
            let media_hash: [u8; 32] = Sha256::digest(config.json().to_string().as_bytes()).into();
            job.streams[index]
                .bindings
                .iter()
                .map(move |binding| (*binding, media_hash))
        })
        .collect();
    let workers = if has_media {
        media_workers(job.limits.compute_threads).min(media_tasks.len())
    } else {
        0
    };
    let inline = workers == 1;
    config["output"]["thread_allocation"] = json!({
        "requested_budget":job.limits.compute_threads,"coordinator":1,
        "camera_workers":if inline {0} else {workers},
        "codec_worker_allowance":if inline {0} else {workers},
        "codec_threads_per_camera":1,"codec_execution":"synchronous_calling_thread",
        "conservative_compute_max":if inline {1} else {1+2*workers},
        "compression_workers":0});
    if config.to_string().len() > 512 * 1024 {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "resolved config exceeds 512 KiB",
        ));
    }
    let config_hash = format!("{:x}", Sha256::digest(config.to_string().as_bytes()));
    let mut used = BTreeSet::new();
    let mut topics = BTreeSet::from([PROVENANCE_TOPIC.to_owned()]);
    let mut streams = Vec::new();
    for (index, stream) in job.streams.iter().enumerate() {
        let source = sources.metadata_source(&stream.bindings)?;
        if !media.get(index).is_some_and(Option::is_some) {
            validate_independence(sources, stream)?;
        }
        for binding in &stream.bindings {
            used.insert(*binding);
        }
        if !topics.insert(stream.output_topic.clone()) {
            return Err(failure(
                ErrorKind::InvalidConfiguration,
                "conflicting output topic",
                &stream.output_topic,
            ));
        }
        // A single output channel must have unambiguous applicable metadata.
        let first = &sources.files()[stream.bindings[0].file].channels[&stream.bindings[0].channel];
        for binding in &stream.bindings {
            if !media.get(index).is_some_and(Option::is_some)
                && sources.files()[binding.file].channels[&binding.channel].metadata
                    != first.metadata
            {
                return Err(Error::new(
                    ErrorKind::InvalidConfiguration,
                    "logical stream channel metadata conflict",
                ));
            }
        }
        streams.push(Stream {
            source: Box::new(source),
            policy: stream.policy,
            tolerance_ns: stream.tolerance_ns,
            required: stream.required,
        });
    }
    // Each binding repeats stream/source strings in the report. Reserve their
    // JSON values plus worst-case escaping, serialization growth and writer copies
    // before allocating that inventory; configuration size alone does not bound it.
    let report_inventory_bytes: u128 = job
        .streams
        .iter()
        .map(|stream| {
            stream
                .bindings
                .iter()
                .map(|binding| {
                    let file = &sources.files()[binding.file];
                    let channel = &file.channels[&binding.channel];
                    let strings = stream.output_topic.len() as u128
                        + channel.topic.len() as u128
                        + channel.message_encoding.len() as u128
                        + file
                            .schemas
                            .get(&channel.schema_id)
                            .map_or(0, |schema| schema.name.len() as u128);
                    strings * 32 + 8192
                })
                .sum::<u128>()
        })
        .sum();
    if reservation + report_inventory_bytes > 4_294_967_296 {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "report inventory exceeds shared 4 GiB managed pool",
        ));
    }
    let report_inventory_bytes = usize::try_from(report_inventory_bytes).map_err(|error| {
        failure(
            ErrorKind::ResourceLimit,
            "report inventory size overflow",
            error,
        )
    })?;
    let timeline = match &job.timeline {
        TimelineConfig::Fixed { rate, origin } => Timeline::Fixed {
            rate: *rate,
            origin_ns: *origin,
        },
        TimelineConfig::Reference { bindings } => {
            Timeline::Reference(Box::new(sources.metadata_source(bindings)?))
        }
    };
    let mut selector = Selector::new(job.episode, timeline, streams, job.limits.batch_targets)?;
    let media_preparation = Instant::now();
    let mut prepared = if let Some(backend) = backend {
        Some(crate::media::prepare(
            sources,
            &job,
            media,
            backend,
            cancellation,
        )?)
    } else {
        None
    };
    let media_preparation_ns = media_preparation.elapsed().as_nanos();
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::Builder::new()
        .prefix(".foxglove-alignment-")
        .tempfile_in(parent)
        .map_err(write_error)?;
    let output = sources.charged_output(
        staging.reopen().map_err(write_error)?,
        job.limits.max_output_bytes,
        parent,
    );
    let writer = mcap::WriteOptions::new()
        .compression(job.output.compression)
        .compression_level(job.output.zstd_level)
        .compression_threads(0)
        .chunk_size(Some(job.output.chunk_bytes))
        .library("foxglove_mcap_alignment/0.0.0")
        .create(output)
        .map_err(write_error)?;
    let mut writer = GuardedWriter {
        inner: Some(writer),
        failure: sources.write_failure_latch(),
    };
    let mut budget = WriterBudget {
        used: 0,
        limit: job.limits.writer_index_memory_bytes,
    };
    // Channel/schema definitions and retained canonical maps may own several copies.
    for stream in &job.streams {
        let file = &sources.files()[stream.bindings[0].file];
        let descriptor = &file.channels[&stream.bindings[0].channel];
        let schema = file.schemas.get(&descriptor.schema_id);
        let size = descriptor
            .metadata
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
            + descriptor.topic.len()
            + descriptor.message_encoding.len()
            + schema.map_or(0, |s| s.data.len() + s.name.len() + s.encoding.len());
        budget.charge(size.saturating_mul(8).saturating_add(4096))?;
    }
    budget.charge(PROVENANCE_SCHEMA.len() * 8 + config.to_string().len() * 8 + 65536)?;
    // Cover live accumulators, JSON values, serialization and MCAP metadata copies.
    budget.charge(job.streams.len().saturating_mul(32768))?;
    budget.charge(report_inventory_bytes)?;
    let mut output_channels = Vec::new();
    for (index, stream) in job.streams.iter().enumerate() {
        if media.get(index).is_some_and(Option::is_some) {
            let prepared = prepared.as_ref().expect("prepared media channels");
            let mut metadata = None;
            for binding in &stream.bindings {
                if let Some(other) = prepared.channel_metadata(
                    index,
                    *binding,
                    &sources.files()[binding.file].channels[&binding.channel].metadata,
                )? {
                    if metadata.as_ref().is_some_and(|metadata| metadata != &other) {
                        return Err(Error::new(
                            ErrorKind::InvalidConfiguration,
                            "logical stream transformed channel metadata conflict",
                        ));
                    }
                    metadata = Some(other);
                }
            }
            let metadata = metadata.ok_or_else(|| {
                Error::new(
                    ErrorKind::MissingRequired,
                    "media stream has no selected output frames",
                )
            })?;
            output_channels.push(media_channel::<_, foxglove::messages::CompressedImage>(
                &mut writer,
                &stream.output_topic,
                &metadata,
            )?);
        } else {
            output_channels.push(channel(
                sources,
                &mut writer,
                stream.bindings[0],
                &stream.output_topic,
            )?);
        }
    }
    if let Some(prepared) = &prepared {
        for calibration in &prepared.calibrations {
            for (topic, payload) in [
                (&calibration.original_topic, &calibration.original),
                (&calibration.derived_topic, &calibration.derived),
            ] {
                if !topics.insert(topic.clone()) {
                    return Err(failure(
                        ErrorKind::InvalidConfiguration,
                        "calibration output topic conflict",
                        topic,
                    ));
                }
                budget.charge(payload.len().saturating_mul(8).saturating_add(8192))?;
                let binding = Binding {
                    file: calibration.source.identity.file,
                    channel: calibration.source.identity.channel,
                };
                let source_metadata =
                    &sources.files()[binding.file].channels[&binding.channel].metadata;
                let channel = if topic == &calibration.original_topic {
                    channel(sources, &mut writer, binding, topic)?
                } else {
                    let derived = foxglove::messages::CameraCalibration::decode(payload.as_slice())
                        .map_err(|e| {
                            failure(
                                ErrorKind::ValidationFailed,
                                "derived calibration envelope",
                                e,
                            )
                        })?;
                    let metadata = crate::media::transformed_channel_metadata(
                        source_metadata,
                        &json!([derived.width, derived.height]),
                        None,
                    );
                    media_channel::<_, foxglove::messages::CameraCalibration>(
                        &mut writer,
                        topic,
                        &metadata,
                    )?
                };
                writer
                    .write_to_known_channel(
                        &MessageHeader {
                            channel_id: channel,
                            sequence: calibration.source.sequence,
                            log_time: calibration.source.log_time,
                            publish_time: calibration.source.publish_time,
                        },
                        payload,
                    )
                    .map_err(write_error)?;
            }
        }
    }
    let schema = writer
        .add_schema(
            "foxglove.AlignmentProvenanceV1",
            "jsonschema",
            PROVENANCE_SCHEMA.as_bytes(),
        )
        .map_err(write_error)?;
    let provenance_channel = writer
        .add_channel(schema, PROVENANCE_TOPIC, "json", &BTreeMap::new())
        .map_err(write_error)?;
    let source_identities:Vec<_>=sources.files().iter().enumerate().map(|(file,s)|json!({"file":file,"sha256":format!("{:x}",sha2::digest::generic_array::GenericArray::from(s.identity.sha256)),"byte_length":s.identity.byte_length})).collect();
    let run = json!({"contract_version":1,"implementation_version":env!("CARGO_PKG_VERSION"),"config":config,"config_hash":config_hash,"sources":source_identities});
    metadata(&mut writer, RUN_METADATA, &run)?;
    let mut reader = PayloadReader::new(sources, job.limits.cache_bytes);
    let mut context_names = BTreeSet::from([RUN_METADATA.to_owned(), REPORT_METADATA.to_owned()]);
    let mut context_sources = BTreeSet::new();
    if let Some(prepared) = &prepared {
        for calibration in &prepared.calibrations {
            context_sources.insert(Binding {
                file: calibration.source.identity.file,
                channel: calibration.source.identity.channel,
            });
        }
    }
    for context in &job.context {
        check_cancel(cancellation)?;
        match context {
            ContextBinding::Message {
                source,
                output_topic,
            } => {
                sources.verify_message(*source)?;
                let binding = Binding {
                    file: source.identity.file,
                    channel: source.identity.channel,
                };
                if used.contains(&binding) {
                    return Err(Error::new(
                        ErrorKind::InvalidConfiguration,
                        "context binding is also sampled",
                    ));
                }
                let descriptor = &sources.files()[binding.file].channels[&binding.channel];
                let topic = output_topic.as_deref().unwrap_or(&descriptor.topic);
                if !topics.insert(topic.to_owned()) {
                    return Err(failure(
                        ErrorKind::InvalidConfiguration,
                        "context topic conflict; rename required",
                        topic,
                    ));
                }
                let schema_size = sources.files()[binding.file]
                    .schemas
                    .get(&descriptor.schema_id)
                    .map_or(0, |s| s.data.len() + s.name.len() + s.encoding.len());
                budget.charge(
                    schema_size
                        .saturating_add(
                            descriptor
                                .metadata
                                .iter()
                                .map(|(k, v)| k.len() + v.len())
                                .sum::<usize>(),
                        )
                        .saturating_add(topic.len())
                        .saturating_mul(8)
                        .saturating_add(8192),
                )?;
                let id = channel(sources, &mut writer, binding, topic)?;
                let payload = reader.message(*source)?;
                writer
                    .write_to_known_channel(
                        &MessageHeader {
                            channel_id: id,
                            sequence: source.sequence,
                            log_time: source.log_time,
                            publish_time: source.publish_time,
                        },
                        &payload,
                    )
                    .map_err(write_error)?;
                context_sources.insert(binding);
            }
            ContextBinding::Record {
                file,
                offset,
                rename,
            } => {
                sources.verify_outer(*file, *offset)?;
                let (opcode, bytes) = reader.outer(*file, *offset)?;
                match sources.parse_outer(opcode, &bytes, *offset)? {
                    Record::Metadata(mut value) => {
                        if let Some(rename) = rename {
                            value.name.clone_from(rename);
                        }
                        if !context_names.insert(value.name.clone()) {
                            return Err(failure(
                                ErrorKind::InvalidConfiguration,
                                "context name conflict; rename required",
                                value.name,
                            ));
                        }
                        budget.charge(bytes.len().saturating_mul(8).saturating_add(1024))?;
                        writer.write_metadata(&value).map_err(write_error)?;
                    }
                    Record::Attachment {
                        mut header, data, ..
                    } => {
                        if let Some(rename) = rename {
                            header.name.clone_from(rename);
                        }
                        if !context_names.insert(header.name.clone()) {
                            return Err(failure(
                                ErrorKind::InvalidConfiguration,
                                "context name conflict; rename required",
                                header.name,
                            ));
                        }
                        budget.charge(
                            (header.name.len() + header.media_type.len())
                                .saturating_mul(8)
                                .saturating_add(1024),
                        )?;
                        writer
                            .start_attachment(data.len() as u64, header)
                            .map_err(write_error)?;
                        writer.put_attachment_bytes(&data).map_err(write_error)?;
                        writer.finish_attachment().map_err(write_error)?;
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorKind::UnsupportedRecord,
                            "context record must be metadata or attachment",
                        ));
                    }
                }
            }
        }
    }
    let mut stream_statistics = vec![SkewSummary::default(); job.streams.len()];
    let mut selection_ns = prepared.as_ref().map_or(0, |p| p.selection_ns);
    let mut independent_payload_reads_ns = 0u128;
    let mut sampled_writes_ns = 0u128;
    let mut sequences = vec![0u64; job.streams.len()];
    let mut targets = 0u64;
    let mut selections = 0u64;
    let mut largest_sample_payload = 0;
    let mut reused = 0u64;
    let mut selected_records = sources.selected_records()?;
    let mut provenance_sequence = 0u64;
    let planning_ns = started.elapsed().as_nanos().to_string();
    let execution = Instant::now();
    loop {
        let selection = Instant::now();
        let batch = selector.next_batch()?;
        selection_ns += selection.elapsed().as_nanos();
        let Some(batch) = batch else {
            break;
        };
        check_cancel(cancellation)?;
        let mut requests = Vec::new();
        for (target_index, target) in batch.iter().enumerate() {
            for (stream, slot) in target.slots.iter().enumerate() {
                let Some(source) = slot.source else {
                    let mut error = Error::new(
                        ErrorKind::UnsupportedMissingPolicy,
                        "optional missing slots cannot be materialized",
                    );
                    error.target = Some((target.index, target.time));
                    error.stream = Some(stream);
                    return Err(error);
                };
                requests.push((
                    source.identity.file,
                    source.identity.locator.order(),
                    target_index,
                    stream,
                    source,
                ));
            }
        }
        requests.sort_unstable_by_key(|r| (r.0, r.1));
        let scratch =
            tempfile::NamedTempFile::new_in(sources.temporary_directory()?).map_err(write_error)?;
        let mut spool = sources.charged_output(
            scratch.reopen().map_err(write_error)?,
            sources.limits.temporary_bytes,
            &sources.temporary_directory()?,
        );
        let mut positions = vec![(0u64, 0usize); batch.len() * job.streams.len()];
        let mut previous_payload = None;
        let mut media_positions = vec![None; batch.len() * job.streams.len()];
        for (_, _, target, stream, source) in requests {
            if let Some((previous, position, length)) = previous_payload
                && previous == source.identity
                && !media.get(stream).is_some_and(Option::is_some)
            {
                positions[target * job.streams.len() + stream] = (position, length);
                continue;
            }
            check_cancel(cancellation)?;
            let payload = if media.get(stream).is_some_and(Option::is_some) {
                let (payload, metadata) = prepared
                    .as_mut()
                    .expect("prepared media backend")
                    .lookup(stream, source)?;
                let metadata = metadata.to_string();
                let position = spool.stream_position().map_err(write_error)?;
                spool.write_all(metadata.as_bytes()).map_err(write_error)?;
                media_positions[target * job.streams.len() + stream] =
                    Some((position, metadata.len()));
                payload
            } else {
                let read = Instant::now();
                let payload = reader.message(source)?;
                independent_payload_reads_ns += read.elapsed().as_nanos();
                payload
            };
            let position = spool.stream_position().map_err(write_error)?;
            spool.write_all(&payload).map_err(write_error)?;
            positions[target * job.streams.len() + stream] = (position, payload.len());
            previous_payload = if media.get(stream).is_some_and(Option::is_some) {
                None
            } else {
                Some((source.identity, position, payload.len()))
            };
        }
        for (target_position, target) in batch.iter().enumerate() {
            for (stream, slot) in target.slots.iter().enumerate() {
                check_cancel(cancellation)?;
                let source = slot.source.expect("all slots preflighted");
                let sequence =
                    next_sequence(&mut sequences[stream], "output channel sequence exhausted")?;
                let (position, length) = positions[target_position * job.streams.len() + stream];
                largest_sample_payload = largest_sample_payload.max(length);
                spool.seek(SeekFrom::Start(position)).map_err(write_error)?;
                let mut payload = vec![0u8; length];
                spool.read_exact(&mut payload).map_err(write_error)?;
                budget.charge(2048)?;
                let write = Instant::now();
                writer
                    .write_to_known_channel(
                        &MessageHeader {
                            channel_id: output_channels[stream],
                            sequence,
                            log_time: target.time,
                            publish_time: source.publish_time,
                        },
                        &payload,
                    )
                    .map_err(write_error)?;
                sampled_writes_ns += write.elapsed().as_nanos();
                let mut provenance = json!({"contract_version":1,"implementation_version":env!("CARGO_PKG_VERSION"),"config_hash":config_hash,"target_index":target.index,"target_time":target.time.to_string(),"stream":stream,"output_topic":job.streams[stream].output_topic,"output_channel":output_channels[stream],"source_file":source.identity.file,"source_channel":source.identity.channel,"source_locator":locator_json(source.identity.locator),"source_log_time":source.log_time.to_string(),"source_publish_time":source.publish_time.to_string(),"source_sequence":source.sequence,"offset_ns":slot.offset_ns.expect("valid slot offset").to_string(),"valid":true,"reused":slot.reused,"output_sequence":sequence});
                if let Some((position, length)) =
                    media_positions[target_position * job.streams.len() + stream]
                {
                    spool.seek(SeekFrom::Start(position)).map_err(write_error)?;
                    let mut bytes = vec![0; length];
                    spool.read_exact(&mut bytes).map_err(write_error)?;
                    provenance["media"] = serde_json::from_slice(&bytes).map_err(|e| {
                        failure(ErrorKind::ValidationFailed, "spooled media provenance", e)
                    })?;
                }
                let psequence =
                    next_sequence(&mut provenance_sequence, "provenance sequence exhausted")?;
                writer
                    .write_to_known_channel(
                        &MessageHeader {
                            channel_id: provenance_channel,
                            sequence: psequence,
                            log_time: target.time,
                            publish_time: source.publish_time,
                        },
                        provenance.to_string().as_bytes(),
                    )
                    .map_err(write_error)?;
                stream_statistics[stream]
                    .observe(slot.offset_ns.expect("valid slot offset"), slot.reused);
                selections += 1;
                if slot.reused {
                    reused += 1;
                } else {
                    selected_records.push(source)?;
                }
            }
            targets += 1;
        }
        drop(spool);
        drop(scratch);
    }
    let excluded: Vec<_> = sources
        .files()
        .iter()
        .enumerate()
        .flat_map(|(file, inventory)| {
            inventory.channels.keys().map(move |channel| Binding {
                file,
                channel: *channel,
            })
        })
        .filter(|binding| !used.contains(binding) && !context_sources.contains(binding))
        .map(|b| json!({"file":b.file,"channel":b.channel}))
        .collect();
    let unique = selected_records.finish()?;
    let count = sources.bound_count(&used.iter().copied().collect::<Vec<_>>());
    let input_bytes = sources.files().iter().try_fold(0u64, |sum, file| {
        sum.checked_add(file.identity.byte_length)
            .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow, "total input bytes overflow"))
    })?;
    let sampled_channels: Vec<_> = job
        .streams
        .iter()
        .enumerate()
        .flat_map(|(stream, config)| {
            config.bindings.iter().map(move |binding| {
                let file = &sources.files()[binding.file];
                let channel = &file.channels[&binding.channel];
                json!({"stream":stream,"file":binding.file,"channel":binding.channel,
                "output_topic":config.output_topic,"source_topic":channel.topic,
                "message_encoding":channel.message_encoding,
                "schema_name":file.schemas.get(&channel.schema_id).map(|schema| &schema.name)})
            })
        })
        .collect();
    let stream_statistics: Vec<_> = stream_statistics
        .iter()
        .enumerate()
        .map(|(stream, stats)| stats.json(stream))
        .collect();
    let mut report = json!({"contract_version":1,"implementation_version":env!("CARGO_PKG_VERSION"),"config_hash":config_hash,"targets":targets,"selections":selections,"reused":reused,"unique_selected":unique,"unique_unselected":count-unique,"missing_slots":0,"failures":0,"excluded_channels":excluded,"read_bytes":reader.read_bytes+prepared.as_ref().map_or(0,|p|p.read_bytes),"decompressions":reader.decompressions+prepared.as_ref().map_or(0,|p|p.decompressions),"cache_hits":reader.cache_hits+prepared.as_ref().map_or(0,|p|p.cache_hits),"output_bytes_before_report":std::fs::metadata(staging.path()).map_err(write_error)?.len(),"peak_temporary_bytes_before_finalize":sources.storage_peak()?,"planning_ns":planning_ns,"execution_ns":execution.elapsed().as_nanos().to_string(),"media_dependency_frames":prepared.as_ref().map_or(0,|p|p.dependency_frames),"media_decoded_frames":prepared.as_ref().map_or(0,|p|p.decoded_frames),"media_video_packets":prepared.as_ref().map_or(0,|p|p.packets),"image_encodes":prepared.as_ref().map_or(0,|p|p.encodes),"image_encode_reuse":prepared.as_ref().map_or(0,|p|p.selected_uses-p.encodes),"media_capabilities":prepared.as_ref().map(|p|&p.capabilities),"writer_index_memory_charge":budget.used});
    report["output"] = config["output"].clone();
    report["input_bytes"] = json!(input_bytes);
    report["sampled_channels"] = json!(sampled_channels);
    report["stream_statistics"] = json!(stream_statistics);
    report["skew_histogram"] = json!({"bins":129,"zero_index":64,
        "negative_index":"63-k","positive_index":"65+k","magnitude_bounds_ns":"[2^k,2^(k+1)-1], k=0..63",
        "offset":"source_log_time-target_time","mean":"exact mean_numerator/mean_denominator; denominator 0 means no matches"});
    report["stage_wall_ns"] = json!({"media_preparation":media_preparation_ns.to_string(),
        "selection":selection_ns.to_string(),"independent_payload_reads":independent_payload_reads_ns.to_string(),
        "sampled_writes":sampled_writes_ns.to_string()});
    report["timing_scope"] = json!(
        "Elapsed wall nanoseconds, not CPU sums. Planning includes media preparation and setup; execution includes main selection, independent payload reads, spool I/O, sampled/provenance writes and selected-identity accounting. Selection includes media planning passes and main passes and overlaps media_preparation. Media preparation includes concurrent codec workers as one enclosing wall interval. Independent payload reads time only source payload fetches; sampled writes time only sampled output message writes. Stages are partial and overlapping, not additive. Finalization, validation and publication are returned separately."
    );
    metadata(&mut writer, REPORT_METADATA, &report)?;
    let finalization = Instant::now();
    let summary = writer.finish().map_err(write_error)?;
    let output = writer.into_inner();
    output.sync()?;
    let finalization_ns = finalization.elapsed().as_nanos();
    let validation = Instant::now();
    check_cancel(cancellation)?;
    sources
        .validate_output(
            staging.path(),
            job.limits.writer_index_memory_bytes,
            largest_sample_payload,
            &summary.chunk_indexes,
        )
        .map_err(|mut e| {
            if e.kind != ErrorKind::ResourceLimit {
                e.kind = ErrorKind::ValidationFailed;
            }
            e
        })?;
    sources.validate_sources()?;
    check_cancel(cancellation)?;
    let validation_ns = validation.elapsed().as_nanos();
    let output_bytes = std::fs::metadata(staging.path())
        .map_err(write_error)?
        .len();
    let peak_temporary_bytes = sources.storage_peak()?;
    let publication = Instant::now();
    std::fs::hard_link(staging.path(), destination).map_err(|e| {
        failure(
            ErrorKind::IoFailed,
            "no-clobber atomic MCAP publication",
            format!("{}: {e}", destination.display()),
        )
    })?;
    drop(output);
    Ok(Report {
        json: report,
        output_bytes,
        peak_temporary_bytes,
        finalization_ns,
        validation_ns,
        publication_ns: publication.elapsed().as_nanos(),
    })
}

fn next_sequence(counter: &mut u64, context: &'static str) -> Result<u32, Error> {
    let sequence =
        u32::try_from(*counter).map_err(|e| failure(ErrorKind::ArithmeticOverflow, context, e))?;
    *counter = counter
        .checked_add(1)
        .ok_or_else(|| Error::new(ErrorKind::ArithmeticOverflow, context))?;
    Ok(sequence)
}

struct WriterBudget {
    used: usize,
    limit: usize,
}
impl WriterBudget {
    fn charge(&mut self, bytes: usize) -> Result<(), Error> {
        self.used = self.used.checked_add(bytes).ok_or_else(|| {
            Error::new(ErrorKind::ResourceLimit, "writer index accounting overflow")
        })?;
        if self.used > self.limit {
            Err(failure(
                ErrorKind::ResourceLimit,
                "writer index memory exhausted",
                format!("required {}, limit {}", self.used, self.limit),
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skew_summary_preserves_signed_bin_boundaries_and_exact_mean() {
        let mut stats = SkewSummary::default();
        stats.observe(0, false);
        for exponent in 0..64 {
            let low = 1i128 << exponent;
            let high = (1i128 << (exponent + 1)) - 1;
            for magnitude in [low, high] {
                stats.observe(-magnitude, false);
                stats.observe(magnitude, true);
            }
        }
        let value = stats.json(3);
        assert_eq!(value["accepted"], 257);
        assert_eq!(value["reused"], 128);
        assert_eq!(value["skew_ns"]["min"], format!("-{}", u64::MAX));
        assert_eq!(value["skew_ns"]["max"], u64::MAX.to_string());
        assert_eq!(value["skew_ns"]["mean_numerator"], "0");
        assert_eq!(value["skew_ns"]["mean_denominator"], "257");
        for (index, count) in stats.histogram.iter().enumerate() {
            assert_eq!(*count, if index == 64 { 1 } else { 2 });
        }
        stats.observe(-3, true);
        assert_eq!(stats.json(3)["skew_ns"]["mean_numerator"], "-3");
    }

    #[test]
    fn skew_summary_represents_empty_and_larger_than_signed_128_sums() {
        let empty = SkewSummary::default().json(0);
        assert!(empty["skew_ns"]["min"].is_null());
        assert!(empty["skew_ns"]["max"].is_null());
        assert_eq!(empty["skew_ns"]["mean_denominator"], "0");
        let mut stats = SkewSummary {
            accepted: u64::MAX - 1,
            positive_sum: u128::from(u64::MAX - 1) * u128::from(u64::MAX),
            ..SkewSummary::default()
        };
        stats.observe(i128::from(u64::MAX), false);
        assert_eq!(
            stats.json(0)["skew_ns"]["mean_numerator"],
            (u128::from(u64::MAX) * u128::from(u64::MAX)).to_string()
        );
        assert_eq!(
            stats.json(0)["skew_ns"]["mean_denominator"],
            u64::MAX.to_string()
        );
    }

    #[test]
    fn writer_destructor_secondary_panic_is_contained_in_a_child() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "executor::tests::writer_secondary_panic_child",
                "--nocapture",
            ])
            .env("FOXGLOVE_ALIGNMENT_SECONDARY_PANIC_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "secondary destructor panic aborted child: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn writer_secondary_panic_child() {
        if std::env::var_os("FOXGLOVE_ALIGNMENT_SECONDARY_PANIC_CHILD").is_none() {
            return;
        }
        use std::sync::atomic::AtomicU64;
        use std::sync::{Arc, Mutex};
        struct Output {
            cursor: std::io::Cursor<Vec<u8>>,
            position: Arc<AtomicU64>,
            cap: Arc<AtomicU64>,
            failure: Arc<Mutex<Option<Error>>>,
        }
        impl Write for Output {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.cursor.position() + bytes.len() as u64 > self.cap.load(Ordering::Relaxed) {
                    let error = Error::new(
                        ErrorKind::ResourceLimit,
                        "injected chunk initialization write failure",
                    );
                    *self.failure.lock().unwrap() = Some(error.clone());
                    return Err(std::io::Error::other(error));
                }
                let count = self.cursor.write(bytes)?;
                self.position
                    .store(self.cursor.position(), Ordering::Relaxed);
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl Seek for Output {
            fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
                let position = self.cursor.seek(position)?;
                self.position.store(position, Ordering::Relaxed);
                Ok(position)
            }
        }
        let position = Arc::new(AtomicU64::new(0));
        let cap = Arc::new(AtomicU64::new(u64::MAX));
        let failure = Arc::new(Mutex::new(None));
        let output = Output {
            cursor: std::io::Cursor::new(Vec::new()),
            position: Arc::clone(&position),
            cap: Arc::clone(&cap),
            failure: Arc::clone(&failure),
        };
        let writer = mcap::WriteOptions::new()
            .compression(None)
            .create(output)
            .unwrap();
        let guard = GuardedWriter {
            inner: Some(writer),
            failure: Arc::clone(&failure),
        };
        cap.store(position.load(Ordering::Relaxed) + 3, Ordering::Relaxed);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let mut writer = guard;
            assert!(
                writer
                    .add_schema("example.Message", "jsonschema", b"{}")
                    .is_err()
            );
            panic!("original operation panic");
        }))
        .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"original operation panic")
        );
        assert_eq!(
            failure.lock().unwrap().as_ref().unwrap().kind,
            ErrorKind::ResourceLimit
        );
    }

    #[test]
    fn arbitrary_channel_metadata_does_not_classify_numeric_or_declared_sources_as_video() {
        let metadata = BTreeMap::from([
            ("frame_id".into(), "nav1".into()),
            ("notes".into(), "h264 / video/hevc / compressedvideo".into()),
        ]);
        for schema in ["foxglove.Numeric", "custom.Opaque"] {
            assert!(!interframe_video(schema, "json", &metadata));
        }
        assert!(!interframe_video(
            "custom.Av1Diagnostic",
            "json",
            &BTreeMap::new()
        ));
        assert!(!interframe_video(
            "foxglove.CompressedImage",
            "json",
            &BTreeMap::from([("format".into(), "jpeg".into())])
        ));
    }

    #[test]
    fn recognized_video_schemas_serializations_and_exact_codec_fields_remain_dependent() {
        assert!(interframe_video(
            "foxglove.CompressedVideo",
            "protobuf",
            &BTreeMap::new()
        ));
        assert!(interframe_video(
            "foxglove_msgs/msg/CompressedVideo",
            "cdr",
            &BTreeMap::new()
        ));
        assert!(interframe_video("custom.Opaque", "H264", &BTreeMap::new()));
        for (key, value) in [
            ("codec", " h.265 "),
            ("format", "video/H264"),
            ("VIDEO_CODEC", "AV1"),
        ] {
            assert!(interframe_video(
                "custom.Opaque",
                "json",
                &BTreeMap::from([(key.into(), value.into())])
            ));
        }
        assert!(!interframe_video(
            "custom.Opaque",
            "json",
            &BTreeMap::from([("format".into(), "nav1".into())])
        ));
    }

    #[test]
    fn typed_write_errors_preserve_resource_categories_without_matching_text() {
        let inner = Error::new(ErrorKind::ResourceLimit, "bounded allocation");
        assert_eq!(
            write_error(mcap::McapError::Io(std::io::Error::other(inner))).kind,
            ErrorKind::ResourceLimit
        );
        assert_eq!(
            write_error(std::io::Error::from_raw_os_error(28)).kind,
            ErrorKind::ResourceLimit
        );
        assert_eq!(
            write_error(std::io::Error::other("a path containing ResourceLimit")).kind,
            ErrorKind::IoFailed
        );
    }

    #[test]
    fn output_sequence_starts_at_zero_and_fails_instead_of_wrapping() {
        let mut counter = 0;
        assert_eq!(
            next_sequence(&mut counter, "test sequence").expect("initial sequence"),
            0
        );
        counter = u64::from(u32::MAX);
        assert_eq!(
            next_sequence(&mut counter, "test sequence").expect("last legal sequence"),
            u32::MAX
        );
        assert_eq!(
            next_sequence(&mut counter, "test sequence")
                .expect_err("overflow")
                .kind,
            ErrorKind::ArithmeticOverflow
        );
        assert_eq!(counter, u64::from(u32::MAX) + 1);
    }

    #[test]
    fn retained_writer_index_budget_is_independent_of_output_byte_capacity() {
        let mut budget = WriterBudget {
            used: 0,
            limit: 4096,
        };
        budget.charge(2048).expect("first pair");
        budget.charge(2048).expect("second pair");
        assert_eq!(
            budget
                .charge(1)
                .expect_err("retained indexes exceed cap")
                .kind,
            ErrorKind::ResourceLimit
        );
    }
}
