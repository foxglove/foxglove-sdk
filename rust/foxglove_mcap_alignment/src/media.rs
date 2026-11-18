//! Internal media boundary and disk-backed selected-image materialization.
//!
//! The core owns source identities and protobuf envelopes. Codec implementations
//! receive only access units and private ordinal tokens, never MCAP timestamps.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use foxglove::messages::{CameraCalibration, CompressedImage, CompressedVideo};
use prost::Message;
use serde_json::{Value, json};

use crate::executor::{Job, TimelineConfig};
use crate::local_source::payload::{ChargedFile, PayloadReader};
use crate::local_source::{Binding, LocalSources, error};
use crate::{Error, ErrorKind, MetadataSource, Selector, SourceRecord, Stream, Timeline};

/// One encoded image, associated with the exact input access-unit ordinal.
pub struct EncodedFrame {
    pub token: u64,
    pub bytes: Vec<u8>,
    pub metadata: Value,
}
/// A single isolated file/channel decoder. Implementations must reject duplicate,
/// missing and unknown tokens, B pictures and unsupported configuration changes.
pub trait MediaDecoder: Send {
    fn feed(&mut self, data: &[u8], token: u64, selected: bool)
    -> Result<Vec<EncodedFrame>, Error>;
    fn flush(&mut self) -> Result<Vec<EncodedFrame>, Error>;
}
/// Backend-independent boundary; a factory can serve independent camera workers.
/// All codec, color-conversion and image-encoding calls must execute synchronously
/// on their calling thread without spawning internal workers. Implementations
/// must configure software codecs for one thread and disable codec threading.
pub trait MediaProcessor: Sync {
    fn capabilities(&self) -> Result<Value, Error>;
    fn decoder(&self, codec: &str, config: &MediaConfig) -> Result<Box<dyn MediaDecoder>, Error>;
    fn derive_calibration(&self, calibration: &Value, geometry: &Value) -> Result<Value, Error>;
    fn image(
        &self,
        _data: &[u8],
        _format: &str,
        _config: &MediaConfig,
    ) -> Result<EncodedFrame, Error> {
        Err(unsupported("backend does not support independent images"))
    }
}
/// Original and derived calibration channels always have distinct output topics.
#[derive(Clone, Debug)]
pub enum CalibrationConfig {
    Unavailable,
    Bound {
        bindings: Vec<Binding>,
        pixel_coordinates: String,
        original_topic: String,
        derived_topic: String,
    },
}
/// Private per-logical-stream media configuration; PNG/full resolution is default.
#[derive(Clone, Debug)]
pub struct MediaConfig {
    pub resize: Value,
    pub encoding: Value,
    pub calibration: CalibrationConfig,
    pub max_frame_bytes: usize,
}
impl MediaConfig {
    pub fn uncalibrated() -> Self {
        Self {
            resize: json!({"kind":"none"}),
            encoding: json!({"kind":"png"}),
            calibration: CalibrationConfig::Unavailable,
            max_frame_bytes: 64 * 1024 * 1024,
        }
    }
    pub(crate) fn json(&self) -> Value {
        let calibration = match &self.calibration {
            CalibrationConfig::Unavailable => json!({"status":"unavailable"}),
            CalibrationConfig::Bound {
                bindings,
                pixel_coordinates,
                original_topic,
                derived_topic,
            } => {
                json!({"source_bindings":bindings.iter().map(|b|json!({"file":b.file,"channel":b.channel})).collect::<Vec<_>>(),"pixel_coordinates":pixel_coordinates,"original_topic":original_topic,"derived_topic":derived_topic})
            }
        };
        json!({"resize":self.resize,"encoding":self.encoding,"calibration":calibration,"max_frame_bytes":self.max_frame_bytes})
    }
}
fn io(err: impl std::error::Error + 'static) -> Error {
    if let Some(io) = (&err as &dyn std::error::Error).downcast_ref::<std::io::Error>() {
        if let Some(inner) = io.get_ref().and_then(|e| e.downcast_ref::<Error>()) {
            return inner.clone();
        }
        if matches!(io.raw_os_error(), Some(23 | 24 | 28)) {
            return error(ErrorKind::ResourceLimit, "media spool exhausted", io);
        }
    }
    error(ErrorKind::IoFailed, "media spool I/O", err)
}
fn unsupported(detail: impl ToString) -> Error {
    error(
        ErrorKind::UnsupportedMedia,
        "unsupported media source",
        detail,
    )
}
fn cancel(flag: &AtomicBool) -> Result<(), Error> {
    if flag.load(Ordering::Relaxed) {
        Err(Error::new(ErrorKind::Cancelled, "media job cancelled"))
    } else {
        Ok(())
    }
}

pub(crate) fn selector<'a>(sources: &'a LocalSources, job: &Job) -> Result<Selector<'a>, Error> {
    let streams = job
        .streams
        .iter()
        .map(|s| {
            Ok(Stream {
                source: Box::new(sources.metadata_source(&s.bindings)?),
                policy: s.policy,
                tolerance_ns: s.tolerance_ns,
                required: s.required,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let timeline = match &job.timeline {
        TimelineConfig::Fixed { rate, origin } => Timeline::Fixed {
            rate: *rate,
            origin_ns: *origin,
        },
        TimelineConfig::Reference { bindings } => {
            Timeline::Reference(Box::new(sources.metadata_source(bindings)?))
        }
    };
    Selector::new(job.episode, timeline, streams, job.limits.batch_targets)
}

/// A sorted fixed-width disk index avoids retaining a frame/selection matrix.
struct ImageSpool {
    _data_path: tempfile::NamedTempFile,
    _index_path: tempfile::NamedTempFile,
    data: ChargedFile,
    index: ChargedFile,
    count: u64,
    last_key: Option<(u64, u64)>,
}
impl ImageSpool {
    fn new(sources: &LocalSources) -> Result<Self, Error> {
        let dir = sources.temporary_directory()?;
        let data_path = tempfile::NamedTempFile::new_in(&dir).map_err(io)?;
        let index_path = tempfile::NamedTempFile::new_in(&dir).map_err(io)?;
        Ok(Self {
            data: sources.charged_output(
                data_path.reopen().map_err(io)?,
                sources.limits.temporary_bytes,
                &dir,
            ),
            index: sources.charged_output(
                index_path.reopen().map_err(io)?,
                sources.limits.temporary_bytes,
                &dir,
            ),
            _data_path: data_path,
            _index_path: index_path,
            count: 0,
            last_key: None,
        })
    }
    fn append(
        &mut self,
        source: SourceRecord,
        payload: &[u8],
        metadata: &Value,
    ) -> Result<(), Error> {
        let key = source.identity.locator.order();
        if self.last_key.is_some_and(|last| last >= key) {
            return Err(error(
                ErrorKind::DecodeFailed,
                "decoder output does not preserve unique physical token order",
                "media association",
            ));
        }
        self.last_key = Some(key);
        let position = self.data.stream_position().map_err(io)?;
        let meta = metadata.to_string();
        self.data
            .write_all(&(meta.len() as u64).to_le_bytes())
            .map_err(io)?;
        self.data.write_all(meta.as_bytes()).map_err(io)?;
        self.data.write_all(payload).map_err(io)?;
        let (a, b) = source.identity.locator.order();
        for value in [a, b, position, payload.len() as u64] {
            self.index.write_all(&value.to_le_bytes()).map_err(io)?;
        }
        self.count += 1;
        Ok(())
    }
    fn metadata(&mut self) -> Result<Option<Value>, Error> {
        if self.count == 0 {
            return Ok(None);
        }
        self.data.seek(SeekFrom::Start(0)).map_err(io)?;
        let mut length = [0; 8];
        self.data.read_exact(&mut length).map_err(io)?;
        let mut bytes = vec![0; u64::from_le_bytes(length) as usize];
        self.data.read_exact(&mut bytes).map_err(io)?;
        Ok(Some(serde_json::from_slice(&bytes).map_err(io)?))
    }
    fn lookup(&mut self, source: SourceRecord) -> Result<(Vec<u8>, Value), Error> {
        let key = source.identity.locator.order();
        let (mut low, mut high) = (0, self.count);
        while low < high {
            let mid = low + (high - low) / 2;
            self.index.seek(SeekFrom::Start(mid * 32)).map_err(io)?;
            let mut bytes = [0u8; 32];
            self.index.read_exact(&mut bytes).map_err(io)?;
            let at = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().expect("fixed field"));
            match (at(0), at(8)).cmp(&key) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    self.data.seek(SeekFrom::Start(at(16))).map_err(io)?;
                    let mut length = [0u8; 8];
                    self.data.read_exact(&mut length).map_err(io)?;
                    let mut meta = vec![0; u64::from_le_bytes(length) as usize];
                    self.data.read_exact(&mut meta).map_err(io)?;
                    let mut payload = vec![0; at(24) as usize];
                    self.data.read_exact(&mut payload).map_err(io)?;
                    return Ok((payload, serde_json::from_slice(&meta).map_err(io)?));
                }
            }
        }
        Err(error(
            ErrorKind::ValidationFailed,
            "selected encoded frame missing",
            format!("{:?}", source.identity),
        ))
    }
}

/// Preserve application metadata; only recognized media fields describe changed pixels.
/// The complete original map is also retained in each image's source provenance.
pub(crate) fn transformed_channel_metadata(
    original: &BTreeMap<String, String>,
    dimensions: &Value,
    encoding: Option<&str>,
) -> BTreeMap<String, String> {
    let mut result = original.clone();
    for (key, value) in &mut result {
        match key.to_ascii_lowercase().as_str() {
            "codec"
            | "video_codec"
            | "media_format"
            | "format"
            | "encoding"
            | "foxglove.codec"
            | "foxglove.media_format" => {
                if let Some(encoding) = encoding {
                    *value = encoding.into();
                }
            }
            "width" | "image_width" | "foxglove.width" => {
                if let Some(width) = dimensions[0].as_u64() {
                    *value = width.to_string();
                }
            }
            "height" | "image_height" | "foxglove.height" => {
                if let Some(height) = dimensions[1].as_u64() {
                    *value = height.to_string();
                }
            }
            _ => {}
        }
    }
    result
}
pub(crate) struct CalibrationOutput {
    pub source: SourceRecord,
    pub original_topic: String,
    pub derived_topic: String,
    pub original: Vec<u8>,
    pub derived: Vec<u8>,
}
pub(crate) struct PreparedMedia {
    spools: BTreeMap<(usize, Binding), Arc<Mutex<ImageSpool>>>,
    pub calibrations: Vec<CalibrationOutput>,
    contributions: BTreeSet<(usize, Binding)>,
    calibration_status: BTreeMap<(usize, Binding), Value>,
    pub packets: u64,
    pub decoded_frames: u64,
    pub dependency_frames: u64,
    pub encodes: u64,
    pub selected_uses: u64,
    pub selection_ns: u128,
    pub capabilities: Value,
    pub read_bytes: u64,
    pub decompressions: u64,
    pub cache_hits: u64,
    _handles: crate::local_source::CursorLease,
}
impl PreparedMedia {
    pub fn channel_metadata(
        &self,
        stream: usize,
        binding: Binding,
        original: &BTreeMap<String, String>,
    ) -> Result<Option<BTreeMap<String, String>>, Error> {
        if !self.contributions.contains(&(stream, binding)) {
            return Ok(None);
        }
        let spool = self
            .spools
            .get(&(stream, binding))
            .ok_or_else(|| unsupported("missing media channel binding"))?;
        let metadata = spool
            .lock()
            .map_err(|_| unsupported("encoded spool lock poisoned"))?
            .metadata()?;
        Ok(metadata.map(|metadata| {
            transformed_channel_metadata(
                original,
                &metadata["geometry"]["output_dimensions"],
                metadata["encoding"]["kind"].as_str(),
            )
        }))
    }

    pub fn lookup(
        &mut self,
        stream: usize,
        source: SourceRecord,
    ) -> Result<(Vec<u8>, Value), Error> {
        self.selected_uses += 1;
        let binding = Binding {
            file: source.identity.file,
            channel: source.identity.channel,
        };
        let (payload, mut metadata) = self
            .spools
            .get_mut(&(stream, binding))
            .ok_or_else(|| unsupported("missing media binding"))?
            .lock()
            .map_err(|_| Error::new(ErrorKind::ValidationFailed, "encoded spool lock poisoned"))?
            .lookup(source)?;
        metadata["calibration"] = self
            .calibration_status
            .get(&(stream, binding))
            .ok_or_else(|| unsupported("selected frame lacks logical calibration status"))?
            .clone();
        Ok((payload, metadata))
    }
}

#[derive(Default)]
struct MediaReadStats {
    read_bytes: u64,
    decompressions: u64,
    cache_hits: u64,
}
#[allow(clippy::too_many_arguments)]
fn calibration(
    sources: &LocalSources,
    config: &MediaConfig,
    binding: Binding,
    frame_id: &str,
    geometry: &Value,
    backend: &dyn MediaProcessor,
    cache_bytes: usize,
    reads: &mut MediaReadStats,
) -> Result<Option<(CalibrationOutput, Value)>, Error> {
    let mut reader = PayloadReader::new(sources, cache_bytes);
    let result = match &config.calibration {
        CalibrationConfig::Unavailable => {
            let file = &sources.files()[binding.file];
            for (id, channel) in &file.channels {
                let name = file
                    .schemas
                    .get(&channel.schema_id)
                    .map_or("", |s| s.name.as_str());
                if name.to_ascii_lowercase().contains("cameracalibration")
                    || name.contains("CameraInfo")
                {
                    if name != "foxglove.CameraCalibration"
                        || channel.message_encoding != "protobuf"
                    {
                        return Err(unsupported(
                            "available calibration requires an explicit supported binding",
                        ));
                    }
                    let mut cursor = sources.metadata_source(&[Binding {
                        file: binding.file,
                        channel: *id,
                    }])?;
                    while let Some(source) = cursor.next_record()? {
                        let decoded = CameraCalibration::decode(reader.message(source)?.as_slice())
                            .map_err(unsupported)?;
                        if decoded.frame_id == frame_id || decoded.frame_id.is_empty() {
                            return Err(unsupported(
                                "calibration unavailable declared for a camera with available calibration",
                            ));
                        }
                    }
                }
            }
            Ok(None)
        }
        CalibrationConfig::Bound {
            bindings,
            pixel_coordinates,
            original_topic,
            derived_topic,
        } => {
            if pixel_coordinates != "integer_centers" || original_topic == derived_topic {
                return Err(unsupported(
                    "calibration requires integer_centers and distinct original/derived topics",
                ));
            }
            let matching = bindings
                .iter()
                .filter(|b| b.file == binding.file)
                .copied()
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(unsupported(
                    "ambiguous or missing calibration binding for camera source file",
                ));
            }
            let bound = matching[0];
            let file = &sources.files()[bound.file];
            let descriptor = file
                .channels
                .get(&bound.channel)
                .ok_or_else(|| unsupported("unknown calibration channel"))?;
            if descriptor.message_encoding != "protobuf"
                || file
                    .schemas
                    .get(&descriptor.schema_id)
                    .map(|s| s.name.as_str())
                    != Some("foxglove.CameraCalibration")
            {
                return Err(unsupported("unsupported calibration schema/serialization"));
            }
            let mut cursor = sources.metadata_source(&[bound])?;
            let mut first = None;
            let mut normalized = None;
            while let Some(source) = cursor.next_record()? {
                let payload = reader.message(source)?;
                let decoded = CameraCalibration::decode(payload.as_slice()).map_err(unsupported)?;
                if decoded.frame_id != frame_id {
                    return Err(unsupported(
                        "calibration frame_id differs from video frame_id",
                    ));
                }
                let mut value = json!({"width":decoded.width,"height":decoded.height,"distortion_model":decoded.distortion_model,"D":decoded.d,"K":decoded.k,"frame_id":decoded.frame_id,"pixel_coordinates":pixel_coordinates});
                if !decoded.r.is_empty() {
                    value["R"] = json!(decoded.r);
                }
                if !decoded.p.is_empty() {
                    value["P"] = json!(decoded.p);
                }
                if normalized.as_ref().is_some_and(|n| n != &value) {
                    return Err(unsupported("time-varying calibration"));
                }
                normalized = Some(value);
                if first.is_none() {
                    first = Some((source, payload, decoded));
                }
            }
            let (source, original, mut decoded) = first.ok_or_else(|| {
                Error::new(ErrorKind::MissingRequired, "bound calibration is absent")
            })?;
            let derived = backend
                .derive_calibration(normalized.as_ref().expect("first calibration"), geometry)?;
            decoded.width = derived["width"]
                .as_u64()
                .and_then(|x| u32::try_from(x).ok())
                .ok_or_else(|| unsupported("invalid derived width"))?;
            decoded.height = derived["height"]
                .as_u64()
                .and_then(|x| u32::try_from(x).ok())
                .ok_or_else(|| unsupported("invalid derived height"))?;
            let matrix = |name: &str| -> Result<Vec<f64>, Error> {
                derived[name]
                    .as_array()
                    .ok_or_else(|| unsupported("missing derived matrix"))?
                    .iter()
                    .map(|v| {
                        v.as_f64()
                            .ok_or_else(|| unsupported("invalid derived matrix"))
                    })
                    .collect()
            };
            decoded.k = matrix("K")?;
            if !decoded.p.is_empty() {
                decoded.p = matrix("P")?;
            }
            let provenance = json!({"status":"derived","adapter":"foxglove.CameraCalibration/pinhole-v1","pixel_coordinates":pixel_coordinates,"source_file":source.identity.file,"source_channel":source.identity.channel,"source_log_time":source.log_time.to_string(),"source_publish_time":source.publish_time.to_string(),"source_sequence":source.sequence,"source_locator":crate::executor::locator_json(source.identity.locator),"original_topic":original_topic,"derived_topic":derived_topic});
            Ok(Some((
                CalibrationOutput {
                    source,
                    original_topic: original_topic.clone(),
                    derived_topic: derived_topic.clone(),
                    original,
                    derived: decoded.encode_to_vec(),
                },
                provenance,
            )))
        }
    };
    reads.read_bytes += reader.read_bytes;
    reads.decompressions += reader.decompressions;
    reads.cache_hits += reader.cache_hits;
    result
}

struct Pending {
    source: SourceRecord,
    envelope: CompressedVideo,
}
fn binding_context(sources: &LocalSources, binding: Binding, codec: &str) -> String {
    let file = &sources.files()[binding.file];
    let channel = &file.channels[&binding.channel];
    format!(
        "file {} ({}), channel {}, topic {}, schema {}, serialization {}, format {}",
        binding.file,
        file.path.display(),
        binding.channel,
        channel.topic,
        file.schemas
            .get(&channel.schema_id)
            .map_or("", |s| s.name.as_str()),
        channel.message_encoding,
        codec
    )
}
fn decoded_frames(
    frames: Vec<EncodedFrame>,
    pending: &mut BTreeMap<u64, Pending>,
    spool: &mut ImageSpool,
    calibration_status: &Value,
) -> Result<(), Error> {
    for mut frame in frames {
        let selected = pending.remove(&frame.token).ok_or_else(|| {
            error(
                ErrorKind::DecodeFailed,
                "decoder emitted an unknown or duplicate selected token",
                "media association",
            )
        })?;
        frame.metadata["source_frame_id"] = json!(selected.envelope.frame_id);
        frame.metadata["calibration"] = calibration_status.clone();
        frame.metadata["decoded_source_token"] = json!(frame.token.to_string());
        if frame.metadata.get("handler").is_none() {
            frame.metadata["handler"] = json!("foxglove.CompressedVideo/protobuf/annex-b-v1");
        }
        let format = frame.metadata["encoding"]["kind"].as_str().ok_or_else(|| {
            error(
                ErrorKind::EncodeFailed,
                "backend omitted output encoding",
                "media encoding",
            )
        })?;
        if !matches!(format, "png" | "jpeg") {
            return Err(error(
                ErrorKind::EncodeFailed,
                "backend returned unsupported output encoding",
                "media encoding",
            ));
        }
        if frame.metadata.to_string().len() > 64 * 1024 {
            return Err(Error::new(
                ErrorKind::ResourceLimit,
                "media metadata exceeds 64 KiB",
            ));
        }
        let image = CompressedImage {
            timestamp: selected.envelope.timestamp,
            frame_id: selected.envelope.frame_id,
            data: frame.bytes.into(),
            format: format.into(),
        };
        spool.append(selected.source, &image.encode_to_vec(), &frame.metadata)?;
    }
    Ok(())
}

pub(crate) fn prepare(
    sources: &LocalSources,
    job: &Job,
    configs: &[Option<MediaConfig>],
    backend: &dyn MediaProcessor,
    cancellation: &AtomicBool,
) -> Result<PreparedMedia, Error> {
    if configs
        .iter()
        .flatten()
        .any(|c| c.resize.get("kind").and_then(Value::as_str) != Some("none"))
    {
        for stream in &job.streams {
            for binding in &stream.bindings {
                let file = &sources.files()[binding.file];
                let channel = &file.channels[&binding.channel];
                let schema = file
                    .schemas
                    .get(&channel.schema_id)
                    .map_or("", |s| s.name.as_str());
                if schema.contains("Annotation") || schema.contains("RegionOfInterest") {
                    return Err(unsupported(
                        "image-coordinate annotation channels must be explicitly excluded when resizing",
                    ));
                }
            }
        }
    }
    let capabilities = backend.capabilities()?;
    let mut tasks = Vec::new();
    let mut reserved = Vec::new();
    let mut aliases = Vec::new();
    for (stream, config) in configs.iter().enumerate() {
        let Some(config) = config else { continue };
        if config.max_frame_bytes == 0 || config.max_frame_bytes > 128 * 1024 * 1024 {
            return Err(Error::new(
                ErrorKind::InvalidConfiguration,
                "media frame reservation must be between 1 byte and 128 MiB",
            ));
        }
        for binding in &job.streams[stream].bindings {
            let file = &sources.files()[binding.file];
            let channel = &file.channels[&binding.channel];
            let schema = file.schemas.get(&channel.schema_id);
            if !matches!(
                schema.map(|s| s.name.as_str()),
                Some("foxglove.CompressedVideo" | "foxglove.CompressedImage")
            ) || channel.message_encoding != "protobuf"
                || schema.map(|s| s.encoding.as_str()) != Some("protobuf")
            {
                return Err(unsupported(binding_context(
                    sources,
                    *binding,
                    "unresolved",
                )));
            }
            if let Some((canonical, _, _)) =
                tasks
                    .iter()
                    .find(|(_, b, c): &&(usize, Binding, &MediaConfig)| {
                        b == binding && c.json() == config.json()
                    })
            {
                aliases.push((stream, *binding, *canonical));
                continue;
            }
            tasks.push((stream, *binding, config));
            reserved.push(
                config.max_frame_bytes as u128 * 4
                    + job.limits.cache_bytes as u128
                    + sources.limits.max_decompressed_chunk_bytes as u128 * 3
                    + sources.limits.max_record_bytes as u128,
            );
        }
    }
    reserved.sort_unstable_by(|a, b| b.cmp(a));
    let additional = reserved
        .iter()
        .take(crate::executor::media_workers(job.limits.compute_threads))
        .sum::<u128>();
    let base = sources.managed_bytes as u128
        + job.limits.writer_index_memory_bytes as u128
        + job.limits.cache_bytes as u128
        + job.limits.batch_targets as u128 * job.streams.len() as u128 * 512
        + 128 * 1024 * 1024
        + job.output.workspace_bytes()
        + sources.limits.max_decompressed_chunk_bytes as u128 * 3;
    if base + additional > 4_294_967_296 {
        return Err(Error::new(
            ErrorKind::ResourceLimit,
            "parallel media reservations exceed shared 4 GiB managed pool",
        ));
    }
    let handles = sources.reserve_execution_handles(
        tasks
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add(6))
            .ok_or_else(|| Error::new(ErrorKind::ResourceLimit, "media spool handle overflow"))?,
    )?;
    // Each configuration owns a union of exactly its logical selections. Distinct
    // transforms on the same binding must not encode each other's requested frames.
    let mut selection_ns = 0u128;
    let mut selected_sets = BTreeMap::new();
    let mut contributions = BTreeSet::new();
    for &(_, _, config) in &tasks {
        let value = config.json();
        let key = value.to_string();
        if selected_sets.contains_key(&key) {
            continue;
        }
        let matches = configs
            .iter()
            .map(|c| c.as_ref().is_some_and(|c| c.json() == value))
            .collect::<Vec<_>>();
        let mut selected = sources.selected_physical_records()?;
        let mut selection = selector(sources, job)?;
        loop {
            let started = Instant::now();
            let batch = selection.next_batch()?;
            selection_ns += started.elapsed().as_nanos();
            let Some(batch) = batch else {
                break;
            };
            cancel(cancellation)?;
            for target in batch {
                for (stream, slot) in target.slots.into_iter().enumerate() {
                    if matches[stream] {
                        let source = slot.source.ok_or_else(|| {
                            Error::new(
                                ErrorKind::UnsupportedMissingPolicy,
                                "media optional miss cannot be materialized",
                            )
                        })?;
                        contributions.insert((
                            stream,
                            Binding {
                                file: source.identity.file,
                                channel: source.identity.channel,
                            },
                        ));
                        selected.push(source)?;
                    }
                }
            }
        }
        drop(selection);
        selected_sets.insert(key, selected.finish_cursor()?);
    }
    let mut result = PreparedMedia {
        spools: BTreeMap::new(),
        calibrations: Vec::new(),
        contributions,
        calibration_status: BTreeMap::new(),
        packets: 0,
        decoded_frames: 0,
        dependency_frames: 0,
        encodes: 0,
        selected_uses: 0,
        selection_ns,
        capabilities,
        read_bytes: 0,
        decompressions: 0,
        cache_hits: 0,
        _handles: handles,
    };
    let mut calibration_bytes = 0u128;
    for group in tasks.chunks(crate::executor::media_workers(job.limits.compute_threads)) {
        // A single worker runs on the coordinator, so budget one creates no threads.
        let outputs = if group.len() == 1 {
            let &(stream, binding, config) = &group[0];
            let selected = selected_sets
                .get(&config.json().to_string())
                .expect("selected configuration index")
                .fork()?;
            vec![process_binding(
                sources,
                job,
                stream,
                binding,
                config,
                backend,
                cancellation,
                selected,
            )?]
        } else {
            std::thread::scope(|scope| -> Result<Vec<_>, Error> {
                let mut workers = Vec::new();
                for &(stream, binding, config) in group {
                    let selected = selected_sets
                        .get(&config.json().to_string())
                        .expect("selected configuration index")
                        .fork()?;
                    workers.push(scope.spawn(move || {
                        process_binding(
                            sources,
                            job,
                            stream,
                            binding,
                            config,
                            backend,
                            cancellation,
                            selected,
                        )
                    }));
                }
                workers
                    .into_iter()
                    .map(|worker| {
                        worker.join().map_err(|_| {
                            Error::new(ErrorKind::ValidationFailed, "camera worker panicked")
                        })?
                    })
                    .collect()
            })?
        };
        for output in outputs {
            result.encodes += output.spool.count;
            result.packets += output.packets;
            result.decoded_frames += output.decoded_frames;
            result.dependency_frames += output.dependency_frames;
            result.read_bytes += output.read_bytes;
            result.decompressions += output.decompressions;
            result.cache_hits += output.cache_hits;
            result.spools.insert(
                (output.stream, output.binding),
                Arc::new(Mutex::new(output.spool)),
            );
            // Workers validate source calibration immediately. Logical output binding
            // names are resolved separately from shared encoded image ownership.
            drop(output.calibration);
        }
    }
    for (stream, binding, canonical) in aliases {
        let spool = Arc::clone(
            result
                .spools
                .get(&(canonical, binding))
                .expect("prepared canonical binding"),
        );
        result.spools.insert((stream, binding), spool);
    }
    let mut calibration_reads = MediaReadStats::default();
    for &(stream, binding) in &result.contributions {
        cancel(cancellation)?;
        let metadata = result
            .spools
            .get(&(stream, binding))
            .expect("contributing encoded binding")
            .lock()
            .map_err(|_| unsupported("encoded spool lock poisoned"))?
            .metadata()?
            .ok_or_else(|| unsupported("contributing binding has no encoded frame"))?;
        let config = resolved_config(
            job,
            stream,
            binding,
            configs[stream].as_ref().expect("media config"),
        );
        let frame_id = metadata["source_frame_id"]
            .as_str()
            .ok_or_else(|| unsupported("encoded frame lacks source frame identity"))?;
        let status = if let Some((output, status)) = calibration(
            sources,
            &config,
            binding,
            frame_id,
            &metadata["geometry"],
            backend,
            job.limits.cache_bytes,
            &mut calibration_reads,
        )? {
            let duplicate = result.calibrations.iter().any(|existing| {
                existing.source == output.source
                    && existing.original_topic == output.original_topic
                    && existing.derived_topic == output.derived_topic
                    && existing.original == output.original
                    && existing.derived == output.derived
            });
            if !duplicate {
                calibration_bytes += (output.original.len() + output.derived.len()) as u128;
                if base + additional + calibration_bytes * 4 > 4_294_967_296 {
                    return Err(Error::new(
                        ErrorKind::ResourceLimit,
                        "retained calibration exceeds shared managed pool",
                    ));
                }
                result.calibrations.push(output);
            }
            status
        } else {
            json!({"status":"unavailable"})
        };
        result.calibration_status.insert((stream, binding), status);
    }
    result.read_bytes += calibration_reads.read_bytes;
    result.decompressions += calibration_reads.decompressions;
    result.cache_hits += calibration_reads.cache_hits;
    Ok(result)
}

fn resolved_config(
    job: &Job,
    stream: usize,
    binding: Binding,
    config: &MediaConfig,
) -> MediaConfig {
    let mut resolved = config.clone();
    if job.streams[stream].bindings.len() > 1
        && let CalibrationConfig::Bound {
            original_topic,
            derived_topic,
            ..
        } = &mut resolved.calibration
    {
        let suffix = format!("/source_{}_{}", binding.file, binding.channel);
        original_topic.push_str(&suffix);
        derived_topic.push_str(&suffix);
    }
    resolved
}

struct BindingOutput {
    stream: usize,
    binding: Binding,
    spool: ImageSpool,
    calibration: Option<CalibrationOutput>,
    packets: u64,
    decoded_frames: u64,
    dependency_frames: u64,
    read_bytes: u64,
    decompressions: u64,
    cache_hits: u64,
}
#[allow(clippy::too_many_arguments)]
fn process_binding(
    sources: &LocalSources,
    job: &Job,
    stream: usize,
    binding: Binding,
    config: &MediaConfig,
    backend: &dyn MediaProcessor,
    cancellation: &AtomicBool,
    mut selected: crate::local_source::PhysicalCursor,
) -> Result<BindingOutput, Error> {
    let context = |codec: &str| binding_context(sources, binding, codec);
    let resolved_config = resolved_config(job, stream, binding, config);
    let config = &resolved_config;
    let mut diagnostic_format = None;
    let result = (|| {
        let file = &sources.files()[binding.file];
        let channel = &file.channels[&binding.channel];
        let image_source = file
            .schemas
            .get(&channel.schema_id)
            .is_some_and(|s| s.name == "foxglove.CompressedImage");
        let mut next_binding_selection = || {
            while let Some(source) = selected.next_record()? {
                if source.identity.file > binding.file {
                    break;
                }
                if source.identity.file == binding.file
                    && source.identity.channel == binding.channel
                {
                    return Ok(Some(source));
                }
            }
            Ok::<_, Error>(None)
        };
        let mut next_selected = next_binding_selection()?;
        let mut spool = ImageSpool::new(sources)?;
        if next_selected.is_none() {
            return Ok(BindingOutput {
                stream,
                binding,
                spool,
                calibration: None,
                packets: 0,
                decoded_frames: 0,
                dependency_frames: 0,
                read_bytes: 0,
                decompressions: 0,
                cache_hits: 0,
            });
        }
        let mut cursor = sources.physical_source(&[binding])?;
        let mut reader = PayloadReader::new(sources, job.limits.cache_bytes);
        let mut decoder = None;
        let mut codec = None;
        let mut frame_id = None;
        let mut pending = BTreeMap::new();
        let mut pending_bytes = 0usize;
        let mut packets = 0u64;
        let mut calibration_output = None;
        let mut calibration_reads = MediaReadStats::default();
        let mut calibration_status = json!({"status":"unavailable"});
        let mut geometry = None;
        while let Some(target) = next_selected {
            let source = cursor
                .next_record()?
                .ok_or_else(|| unsupported("selected media record missing from binding"))?;
            cancel(cancellation)?;
            let wanted = target.identity == source.identity;
            if wanted {
                next_selected = next_binding_selection()?;
            }
            if image_source {
                if !wanted {
                    continue;
                }
                diagnostic_format = None;
                let image = CompressedImage::decode(reader.message(source)?.as_slice())
                    .map_err(unsupported)?;
                diagnostic_format = Some(image.format.clone());
                if frame_id.as_ref().is_some_and(|f| f != &image.frame_id) {
                    return Err(unsupported("midstream image frame_id change"));
                }
                frame_id = Some(image.frame_id.clone());
                let mut frame = backend.image(&image.data, &image.format, config)?;
                cancel(cancellation)?;
                frame.token = packets;
                frame.metadata["handler"] = json!("foxglove.CompressedImage/protobuf/pillow-v1");
                pending.insert(
                    packets,
                    Pending {
                        source,
                        envelope: CompressedVideo {
                            timestamp: image.timestamp,
                            frame_id: image.frame_id,
                            data: Default::default(),
                            format: image.format,
                        },
                    },
                );
                packets += 1;
                consume_frames(
                    sources,
                    config,
                    binding,
                    frame_id.as_deref().unwrap_or(""),
                    backend,
                    job.limits.cache_bytes,
                    vec![frame],
                    &mut pending,
                    &mut spool,
                    &mut geometry,
                    &mut calibration_output,
                    &mut calibration_status,
                    &mut calibration_reads,
                )?;
                continue;
            }
            diagnostic_format = None;
            let envelope =
                CompressedVideo::decode(reader.message(source)?.as_slice()).map_err(unsupported)?;
            diagnostic_format = Some(envelope.format.clone());
            if !matches!(envelope.format.as_str(), "h264" | "h265") {
                return Err(unsupported(context(&envelope.format)));
            }
            if codec.as_ref().is_some_and(|c| c != &envelope.format)
                || frame_id.as_ref().is_some_and(|f| f != &envelope.frame_id)
            {
                return Err(unsupported("midstream codec/frame_id change"));
            }
            if decoder.is_none() {
                decoder = Some(backend.decoder(&envelope.format, config)?);
                codec = Some(envelope.format.clone());
                frame_id = Some(envelope.frame_id.clone());
            }
            let token = packets;
            let frames = decoder.as_mut().expect("initialized decoder").feed(
                &envelope.data,
                token,
                wanted,
            )?;
            cancel(cancellation)?;
            packets = packets.checked_add(1).ok_or_else(|| {
                Error::new(ErrorKind::ArithmeticOverflow, "packet ordinal overflow")
            })?;
            if wanted {
                pending_bytes = pending_bytes
                    .checked_add(envelope.encoded_len())
                    .ok_or_else(|| {
                        Error::new(ErrorKind::ResourceLimit, "pending envelope size overflow")
                    })?;
                if pending_bytes > config.max_frame_bytes || pending.len() >= 128 {
                    return Err(Error::new(
                        ErrorKind::ResourceLimit,
                        "pending source envelope reservation exhausted",
                    ));
                }
                pending.insert(token, Pending { source, envelope });
            }
            consume_frames(
                sources,
                config,
                binding,
                frame_id.as_deref().unwrap_or(""),
                backend,
                job.limits.cache_bytes,
                frames,
                &mut pending,
                &mut spool,
                &mut geometry,
                &mut calibration_output,
                &mut calibration_status,
                &mut calibration_reads,
            )?;
            pending_bytes = pending.values().map(|p| p.envelope.encoded_len()).sum();
        }
        cancel(cancellation)?;
        if let Some(mut decoder) = decoder {
            let frames = decoder.flush()?;
            cancel(cancellation)?;
            consume_frames(
                sources,
                config,
                binding,
                frame_id.as_deref().unwrap_or(""),
                backend,
                job.limits.cache_bytes,
                frames,
                &mut pending,
                &mut spool,
                &mut geometry,
                &mut calibration_output,
                &mut calibration_status,
                &mut calibration_reads,
            )?;
        }
        if !pending.is_empty() {
            return Err(error(
                ErrorKind::DecodeFailed,
                "decoder omitted selected output frames",
                "media association",
            ));
        }
        let dependency_frames = if image_source {
            0
        } else {
            packets.checked_sub(spool.count).ok_or_else(|| {
                error(
                    ErrorKind::DecodeFailed,
                    "encoded more video frames than decoded packets",
                    "media accounting",
                )
            })?
        };
        Ok(BindingOutput {
            stream,
            binding,
            spool,
            calibration: calibration_output,
            packets: if image_source { 0 } else { packets },
            decoded_frames: packets,
            dependency_frames,
            read_bytes: reader.read_bytes + calibration_reads.read_bytes,
            decompressions: reader.decompressions + calibration_reads.decompressions,
            cache_hits: reader.cache_hits + calibration_reads.cache_hits,
        })
    })();
    result.map_err(|mut e: Error| {
        e.detail = Some(format!(
            "{}: {}",
            context(diagnostic_format.as_deref().unwrap_or("per-envelope")),
            e.detail.as_deref().unwrap_or(e.message)
        ));
        e
    })
}

#[allow(clippy::too_many_arguments)]
fn consume_frames(
    sources: &LocalSources,
    config: &MediaConfig,
    binding: Binding,
    frame_id: &str,
    backend: &dyn MediaProcessor,
    cache_bytes: usize,
    mut frames: Vec<EncodedFrame>,
    pending: &mut BTreeMap<u64, Pending>,
    spool: &mut ImageSpool,
    geometry: &mut Option<Value>,
    calibration_output: &mut Option<CalibrationOutput>,
    calibration_status: &mut Value,
    reads: &mut MediaReadStats,
) -> Result<(), Error> {
    if let Some(first) = frames.first() {
        let resolved = first
            .metadata
            .get("geometry")
            .ok_or_else(|| unsupported("backend omitted geometry"))?;
        if geometry.is_none() {
            if let Some((output, status)) = calibration(
                sources,
                config,
                binding,
                frame_id,
                resolved,
                backend,
                cache_bytes,
                reads,
            )? {
                *calibration_output = Some(output);
                *calibration_status = status;
            }
            *geometry = Some(resolved.clone());
        }
        if frames
            .iter()
            .any(|f| Some(&f.metadata["geometry"]) != geometry.as_ref())
        {
            return Err(unsupported("backend image geometry changed"));
        }
    }
    let channel = &sources.files()[binding.file].channels[&binding.channel];
    for frame in &mut frames {
        frame.metadata["source_channel_metadata"] = json!(channel.metadata);
    }
    decoded_frames(frames, pending, spool, calibration_status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{ExecuteLimits, Independence, SampledStream, execute_with_media};
    use crate::local_source::SourceLimits;
    use crate::{Episode, Policy, Rate};
    use foxglove::Encode;
    use std::sync::Arc;

    struct CancellingBackend(Arc<AtomicBool>);
    struct CancellingDecoder(Arc<AtomicBool>);
    impl MediaDecoder for CancellingDecoder {
        fn feed(
            &mut self,
            _data: &[u8],
            _token: u64,
            _selected: bool,
        ) -> Result<Vec<EncodedFrame>, Error> {
            self.0.store(true, Ordering::Relaxed);
            Ok(Vec::new())
        }
        fn flush(&mut self) -> Result<Vec<EncodedFrame>, Error> {
            panic!("cancelled decoder must not flush")
        }
    }
    impl MediaProcessor for CancellingBackend {
        fn capabilities(&self) -> Result<Value, Error> {
            Ok(json!({"backend":"cancellation-test"}))
        }
        fn decoder(
            &self,
            _codec: &str,
            _config: &MediaConfig,
        ) -> Result<Box<dyn MediaDecoder>, Error> {
            Ok(Box::new(CancellingDecoder(Arc::clone(&self.0))))
        }
        fn derive_calibration(
            &self,
            _calibration: &Value,
            _geometry: &Value,
        ) -> Result<Value, Error> {
            panic!("cancelled decoder must not derive calibration")
        }
    }
    #[test]
    fn cancellation_during_media_feed_cleans_temporary_files_and_never_publishes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mcap");
        let mut writer = mcap::Writer::new(std::fs::File::create(&source).unwrap()).unwrap();
        let schema = CompressedVideo::get_schema().unwrap();
        let schema_id = writer
            .add_schema(&schema.name, &schema.encoding, &schema.data)
            .unwrap();
        let channel = writer
            .add_channel(schema_id, "/camera", "protobuf", &BTreeMap::new())
            .unwrap();
        let video = CompressedVideo {
            format: "h264".into(),
            data: vec![0, 0, 0, 1].into(),
            ..Default::default()
        };
        writer
            .write_to_known_channel(
                &mcap::records::MessageHeader {
                    channel_id: channel,
                    sequence: 0,
                    log_time: 1,
                    publish_time: 1,
                },
                &video.encode_to_vec(),
            )
            .unwrap();
        writer.finish().unwrap();
        drop(writer);
        let sources = LocalSources::open(
            std::slice::from_ref(&source),
            SourceLimits {
                index_memory_bytes: 64 * 1024,
                inventory_memory_bytes: 1024 * 1024,
                max_record_bytes: 64 * 1024,
                max_chunk_bytes: 1024 * 1024,
                max_decompressed_chunk_bytes: 1024 * 1024,
                temporary_bytes: 64 * 1024 * 1024,
                max_open_files: 64,
            },
            dir.path(),
        )
        .unwrap();
        let before = sources.storage_peak().unwrap();
        let job = Job {
            output: Default::default(),
            episode: Episode::new(1, 2).unwrap(),
            timeline: TimelineConfig::Fixed {
                rate: Rate::new(1, 1).unwrap(),
                origin: 1,
            },
            streams: vec![SampledStream {
                bindings: vec![Binding { file: 0, channel }],
                output_topic: "/image".into(),
                policy: Policy::Exact,
                tolerance_ns: 0,
                required: true,
                independence: Independence::Declared,
            }],
            context: Vec::new(),
            limits: ExecuteLimits {
                compute_threads: 7,
                batch_targets: 1,
                cache_bytes: 1024 * 1024,
                max_output_bytes: 16 * 1024 * 1024,
                writer_index_memory_bytes: 1024 * 1024,
            },
        };
        let cancellation = Arc::new(AtomicBool::new(false));
        let backend = CancellingBackend(Arc::clone(&cancellation));
        let destination = dir.path().join("output.mcap");
        let error = execute_with_media(
            &sources,
            job,
            &destination,
            &cancellation,
            &[Some(MediaConfig::uncalibrated())],
            &backend,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, ErrorKind::Cancelled);
        assert!(!destination.exists());
        assert!(sources.storage_peak().unwrap() >= before);
        let scratch = sources.temporary_directory().unwrap();
        assert_eq!(
            std::fs::read_dir(scratch).unwrap().count(),
            1,
            "only the immutable snapshot remains"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            2,
            "no sibling output staging remains"
        );
    }
}
