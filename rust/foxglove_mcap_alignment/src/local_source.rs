//! Bounded local MCAP inventory and physical source metadata.
//!
//! Each episode owns a private byte snapshot and sorted in-memory metadata.
//! Paths are never cache keys. Call `validate_sources` again before publishing a job that
//! stops reading before EOF. The snapshot counts against temporary storage.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::{Error, ErrorKind, FileIdentity, MetadataSource, SourceRecord};

pub(crate) mod payload;
mod physical;
pub(crate) use physical::{PhysicalCursor, PhysicalRecords};
mod wire;

/// Explicit limits; chunk buffers and inventory are separate from sort/index memory.
///
/// At most one compressed record and one decompressed chunk coexist. Decoder state,
/// and a 64 KiB copy/hash buffer are fixed overhead, not
/// charged to `index_memory_bytes`. Zstd window memory is capped at 8 MiB; LZ4
/// uses its format-bounded 4 MiB block window. These remain inside the job RSS cap.
#[derive(Clone, Copy, Debug)]
pub struct SourceLimits {
    pub index_memory_bytes: usize,
    pub inventory_memory_bytes: usize,
    pub max_record_bytes: usize,
    pub max_chunk_bytes: usize,
    pub max_decompressed_chunk_bytes: usize,
    pub temporary_bytes: u64,
    pub max_open_files: usize,
}

/// A source stream always includes a configured file ordinal and MCAP channel id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Binding {
    pub file: usize,
    pub channel: u16,
}

/// Schema ids are scoped to a file, independently of topic/channel names.
#[derive(Debug)]
pub struct SchemaDescriptor {
    pub name: String,
    pub encoding: String,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct FileInventory {
    pub path: PathBuf,
    pub identity: FileIdentity,
    pub channels: BTreeMap<u16, mcap::records::Channel>,
    pub schemas: BTreeMap<u16, SchemaDescriptor>,
}

pub(super) fn error(kind: ErrorKind, message: &'static str, detail: impl ToString) -> Error {
    let mut result = Error::new(kind, message);
    result.detail = Some(detail.to_string());
    result
}
fn io_error(err: std::io::Error) -> Error {
    let kind = if matches!(err.raw_os_error(), Some(23 | 24 | 28)) {
        ErrorKind::ResourceLimit
    } else {
        ErrorKind::InvalidSource
    };
    error(kind, "source I/O", err)
}

pub(super) struct Storage {
    directory: TempDir,
    used: u64,
    peak: u64,
    limit: u64,
    serial: u64,
}
impl Storage {
    fn reserve(&mut self, bytes: u64) -> Result<(), Error> {
        self.reserve_bytes(bytes, true)
    }
    fn reserve_bytes(&mut self, bytes: u64, check_free: bool) -> Result<(), Error> {
        let next = self.used.checked_add(bytes).ok_or_else(|| {
            error(
                ErrorKind::ResourceLimit,
                "temporary storage overflow",
                bytes,
            )
        })?;
        if next > self.limit {
            return Err(error(
                ErrorKind::ResourceLimit,
                "temporary storage exhausted (includes snapshots and merge copies)",
                format!("required {next}, cap {}", self.limit),
            ));
        }
        let available = if check_free {
            fs2::available_space(self.directory.path()).map_err(io_error)?
        } else {
            u64::MAX
        };
        if bytes > available {
            return Err(error(
                ErrorKind::ResourceLimit,
                "insufficient free temporary disk space",
                format!("need {bytes}, available {available}"),
            ));
        }
        self.used = next;
        self.peak = self.peak.max(next);
        Ok(())
    }
    fn path(&mut self) -> PathBuf {
        self.serial += 1;
        self.directory.path().join(self.serial.to_string())
    }
}

struct PinnedFile {
    original: File,
    original_metadata: Metadata,
    snapshot: PathBuf,
}
struct EpisodeFiles {
    inventories: Vec<FileInventory>,
    pinned: Mutex<Vec<PinnedFile>>,
    storage: Mutex<Storage>,
    memory: Mutex<CursorBudget>,
    execution: Mutex<()>,
    sort_memory: Mutex<usize>,
    write_failure: Arc<Mutex<Option<Error>>>,
}
struct CursorBudget {
    bytes: usize,
    limit: usize,
    handles: usize,
    handle_limit: usize,
}
pub(crate) struct CursorLease {
    episode: Arc<EpisodeFiles>,
    bytes: usize,
    handles: usize,
}
impl Drop for CursorLease {
    fn drop(&mut self) {
        if let Ok(mut memory) = self.episode.memory.lock() {
            memory.bytes -= self.bytes;
            memory.handles -= self.handles;
        }
    }
}
impl EpisodeFiles {
    fn validate(&self, full: bool) -> Result<(), Error> {
        self.validate_bound(full, None)
    }
    fn validate_bound(&self, full: bool, selected: Option<&BTreeSet<usize>>) -> Result<(), Error> {
        let mut files = self
            .pinned
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "source lock poisoned", "episode"))?;
        for (index, pinned) in files.iter_mut().enumerate() {
            if selected.is_some_and(|selected| !selected.contains(&index)) {
                continue;
            }
            let inventory = &self.inventories[index];
            let current = pinned.original.metadata().map_err(io_error)?;
            let path_current = std::fs::metadata(&inventory.path)
                .map_err(|e| error(ErrorKind::SourceChanged, "source path changed", e))?;
            if !same_metadata(&pinned.original_metadata, &current)
                || !same_metadata(&pinned.original_metadata, &path_current)
                || (full
                    && (hash_file(&mut pinned.original)? != inventory.identity
                        || hash_file(&mut File::open(&pinned.snapshot).map_err(io_error)?)?
                            != inventory.identity))
            {
                return Err(error(
                    ErrorKind::SourceChanged,
                    "source bytes or path version changed",
                    inventory.path.display(),
                ));
            }
        }
        Ok(())
    }
}
fn same_metadata(left: &Metadata, right: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        left.dev() == right.dev()
            && left.ino() == right.ino()
            && left.len() == right.len()
            && left.mtime() == right.mtime()
            && left.mtime_nsec() == right.mtime_nsec()
            && left.ctime() == right.ctime()
            && left.ctime_nsec() == right.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        left.len() == right.len()
            && left.modified().ok() == right.modified().ok()
            && left.created().ok() == right.created().ok()
    }
}
fn hash_file(file: &mut File) -> Result<FileIdentity, Error> {
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut hash = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        length = length
            .checked_add(count as u64)
            .ok_or_else(|| error(ErrorKind::ResourceLimit, "source length overflow", "hash"))?;
    }
    Ok(FileIdentity {
        sha256: hash.finalize().into(),
        byte_length: length,
    })
}

/// Owns the immutable source snapshot and its in-memory metadata index.
/// Cursors retain ownership of the snapshot while traversing the index.
pub struct LocalSources {
    episode: Arc<EpisodeFiles>,
    records: Arc<Vec<SourceRecord>>,
    ranges: BTreeMap<Binding, (usize, usize)>,
    pub(crate) limits: SourceLimits,
    pub(crate) managed_bytes: usize,
}
impl LocalSources {
    pub fn open(
        paths: &[PathBuf],
        limits: SourceLimits,
        temp_parent: &Path,
    ) -> Result<Self, Error> {
        if paths.len() != 1 {
            return Err(Error::new(
                ErrorKind::InvalidConfiguration,
                "the compact example requires one indexed MCAP source",
            ));
        }
        if limits.index_memory_bytes < 4096
            || limits.inventory_memory_bytes < 4096
            || limits.max_record_bytes == 0
            || limits.max_chunk_bytes == 0
            || limits.max_decompressed_chunk_bytes == 0
            || limits.temporary_bytes > 68_719_476_736
            || limits.max_open_files < paths.len().saturating_add(8)
        {
            return Err(error(
                ErrorKind::InvalidConfiguration,
                "invalid source limits",
                "positive limits and nonempty sources required; index/inventory >=4096; temp <=64 GiB",
            ));
        }
        let managed = limits
            .index_memory_bytes
            .checked_mul(2)
            .and_then(|v| v.checked_add(limits.inventory_memory_bytes))
            .and_then(|v| v.checked_add(limits.max_chunk_bytes))
            .and_then(|v| v.checked_add(limits.max_decompressed_chunk_bytes))
            .and_then(|v| {
                limits
                    .max_record_bytes
                    .checked_mul(64)
                    .and_then(|r| v.checked_add(r))
            })
            .and_then(|v| v.checked_add(64 * 1024 * 1024));
        if managed.is_none_or(|bytes| bytes > 4_294_967_296) {
            return Err(error(
                ErrorKind::InvalidConfiguration,
                "source managed limits exceed 4 GiB job pool",
                "sum includes 64x record parser allocation bound and 64MiB fixed overhead",
            ));
        }
        let mut storage = Storage {
            directory: tempfile::Builder::new()
                .prefix("foxglove-alignment-")
                .tempdir_in(temp_parent)
                .map_err(io_error)?,
            used: 0,
            peak: 0,
            limit: limits.temporary_bytes,
            serial: 0,
        };
        let mut inventories = Vec::new();
        let mut pinned = Vec::new();
        let mut records = Vec::new();
        let mut inventory_used = 0usize;
        for path in paths {
            inventory_used = inventory_used
                .checked_add(1024 + path.as_os_str().len())
                .ok_or_else(|| {
                    error(ErrorKind::ResourceLimit, "inventory size overflow", "file")
                })?;
            if inventory_used > limits.inventory_memory_bytes {
                return Err(error(
                    ErrorKind::ResourceLimit,
                    "inventory memory exhausted",
                    path.display(),
                ));
            }
            let mut original = File::open(path).map_err(io_error)?;
            let original_metadata = original.metadata().map_err(io_error)?;
            if !original_metadata.is_file() {
                return Err(error(
                    ErrorKind::InvalidConfiguration,
                    "source must be a regular file",
                    path.display(),
                ));
            }
            storage.reserve(original_metadata.len())?;
            let snapshot = storage.path();
            let mut copy = File::create(&snapshot).map_err(io_error)?;
            let mut hasher = Sha256::new();
            let mut length = 0u64;
            let mut buffer = [0u8; 65536];
            loop {
                let count = original.read(&mut buffer).map_err(io_error)?;
                if count == 0 {
                    break;
                }
                length = length.checked_add(count as u64).ok_or_else(|| {
                    error(
                        ErrorKind::ResourceLimit,
                        "source length overflow",
                        path.display(),
                    )
                })?;
                if length > original_metadata.len() {
                    return Err(error(
                        ErrorKind::SourceChanged,
                        "source grew while snapshotting",
                        path.display(),
                    ));
                }
                hasher.update(&buffer[..count]);
                copy.write_all(&buffer[..count]).map_err(io_error)?;
            }
            copy.flush().map_err(io_error)?;
            let mut permissions = copy.metadata().map_err(io_error)?.permissions();
            permissions.set_readonly(true);
            copy.set_permissions(permissions).map_err(io_error)?;
            drop(copy);
            let identity = FileIdentity {
                sha256: hasher.finalize().into(),
                byte_length: length,
            };
            if identity != hash_file(&mut original)?
                || !same_metadata(&original_metadata, &original.metadata().map_err(io_error)?)
            {
                return Err(error(
                    ErrorKind::SourceChanged,
                    "source changed while snapshotting",
                    path.display(),
                ));
            }
            if inventories
                .iter()
                .any(|entry: &FileInventory| entry.identity == identity)
            {
                return Err(error(
                    ErrorKind::InvalidConfiguration,
                    "same file version configured twice",
                    path.display(),
                ));
            }
            let mut inventory = FileInventory {
                path: path.clone(),
                identity,
                channels: BTreeMap::new(),
                schemas: BTreeMap::new(),
            };
            wire::scan(
                &snapshot,
                &mut inventory,
                &limits,
                Some(&mut records),
                &mut inventory_used,
            )
            .map_err(|mut e| {
                e.detail = Some(format!(
                    "{}: {}",
                    path.display(),
                    e.detail.as_deref().unwrap_or(e.message)
                ));
                e
            })?;
            inventories.push(inventory);
            pinned.push(PinnedFile {
                original,
                original_metadata,
                snapshot,
            });
        }
        records.sort_unstable_by_key(|record| {
            (
                record.identity.channel,
                record.log_time,
                record.identity.order(),
            )
        });
        let mut ranges = BTreeMap::new();
        for (position, record) in records.iter().enumerate() {
            let binding = Binding {
                file: record.identity.file,
                channel: record.identity.channel,
            };
            let range = ranges.entry(binding).or_insert((position, 0));
            range.1 += 1;
        }
        let result = Self {
            episode: Arc::new(EpisodeFiles {
                inventories,
                pinned: Mutex::new(pinned),
                execution: Mutex::new(()),
                sort_memory: Mutex::new(0),
                write_failure: Arc::new(Mutex::new(None)),
                storage: Mutex::new(storage),
                memory: Mutex::new(CursorBudget {
                    bytes: inventory_used,
                    limit: limits.inventory_memory_bytes,
                    handles: paths.len() + 1,
                    handle_limit: limits.max_open_files,
                }),
            }),
            records: Arc::new(records),
            ranges,
            limits,
            managed_bytes: managed.expect("validated managed bound"),
        };
        result.validate_sources()?;
        Ok(result)
    }
    pub fn files(&self) -> &[FileInventory] {
        &self.episode.inventories
    }
    pub fn resolve_topic(&self, topic: &str) -> Result<Binding, Error> {
        let mut found = None;
        for (file, inventory) in self.files().iter().enumerate() {
            for (&channel, descriptor) in &inventory.channels {
                if descriptor.topic == topic {
                    if found.is_some() {
                        return Err(error(
                            ErrorKind::InvalidConfiguration,
                            "ambiguous topic; bind explicit file/channel",
                            topic,
                        ));
                    }
                    found = Some(Binding { file, channel });
                }
            }
        }
        found.ok_or_else(|| error(ErrorKind::InvalidConfiguration, "topic not found", topic))
    }
    /// Multiple bindings may share a stream only with exactly equal schema and serialization.
    pub fn metadata_source(&self, bindings: &[Binding]) -> Result<LocalMetadataSource, Error> {
        if bindings.is_empty() {
            return Err(error(
                ErrorKind::InvalidConfiguration,
                "empty stream bindings",
                "stream",
            ));
        }
        let bytes = bindings
            .len()
            .checked_mul(4608)
            .and_then(|v| v.checked_add(256))
            .ok_or_else(|| {
                error(
                    ErrorKind::ResourceLimit,
                    "cursor memory overflow",
                    "bindings",
                )
            })?;
        let mut memory = self
            .episode
            .memory
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "memory lock poisoned", "episode"))?;
        let next_bytes = memory
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| error(ErrorKind::ResourceLimit, "cursor memory overflow", bytes))?;
        if next_bytes > memory.limit {
            return Err(error(
                ErrorKind::ResourceLimit,
                "live cursor memory budget exhausted",
                format!("bytes {next_bytes}/{}", memory.limit),
            ));
        }
        memory.bytes = next_bytes;
        drop(memory);
        let lease = CursorLease {
            episode: Arc::clone(&self.episode),
            bytes,
            handles: 0,
        };
        let mut resolved = BTreeSet::new();
        let mut descriptor = None;
        for binding in bindings {
            let inventory = self.files().get(binding.file).ok_or_else(|| {
                error(
                    ErrorKind::InvalidConfiguration,
                    "unknown file binding",
                    binding.file,
                )
            })?;
            let channel = inventory.channels.get(&binding.channel).ok_or_else(|| {
                error(
                    ErrorKind::InvalidConfiguration,
                    "unknown channel binding",
                    binding.channel,
                )
            })?;
            let schema = inventory
                .schemas
                .get(&channel.schema_id)
                .map(|s| (&s.name, &s.encoding, &s.data));
            let current = (&channel.message_encoding, schema);
            if descriptor.is_some_and(|previous| previous != current) {
                return Err(error(
                    ErrorKind::InvalidConfiguration,
                    "incompatible schemas/serialization in logical stream",
                    channel.topic.clone(),
                ));
            }
            descriptor = Some(current);
            if !resolved.insert(*binding) {
                return Err(error(
                    ErrorKind::InvalidConfiguration,
                    "duplicate stream binding",
                    binding.channel,
                ));
            }
        }
        let bound_files: BTreeSet<_> = resolved.iter().map(|b| b.file).collect();
        self.episode.validate_bound(false, Some(&bound_files))?;
        let mut cursors = Vec::new();
        cursors
            .try_reserve_exact(resolved.len())
            .map_err(|e| error(ErrorKind::ResourceLimit, "cursor allocation", e))?;
        for binding in resolved {
            let (start, count) = self.ranges.get(&binding).copied().unwrap_or((0, 0));
            cursors.push(RangeCursor {
                start,
                end: start + count,
                position: start,
            });
        }
        Ok(LocalMetadataSource {
            records: Arc::clone(&self.records),
            cursors,
            bound_files,
            _lease: lease,
            episode: Arc::clone(&self.episode),
            ended: false,
            failed: false,
        })
    }
    pub fn validate_sources(&self) -> Result<(), Error> {
        self.episode.validate(true)
    }
    /// The episode directory is exposed for resource/cleanup diagnostics only.
    pub fn temporary_directory(&self) -> Result<PathBuf, Error> {
        Ok(self
            .episode
            .storage
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "storage lock poisoned", "episode"))?
            .directory
            .path()
            .to_path_buf())
    }
}

struct RangeCursor {
    start: usize,
    end: usize,
    position: usize,
}
pub struct LocalMetadataSource {
    records: Arc<Vec<SourceRecord>>,
    cursors: Vec<RangeCursor>,
    bound_files: BTreeSet<usize>,
    _lease: CursorLease,
    episode: Arc<EpisodeFiles>,
    ended: bool,
    failed: bool,
}
impl MetadataSource for LocalMetadataSource {
    fn next_record(&mut self) -> Result<Option<SourceRecord>, Error> {
        if self.failed {
            return Err(error(
                ErrorKind::InvalidSource,
                "source cursor previously failed",
                "terminal",
            ));
        }
        if self.ended {
            return Ok(None);
        }
        let result = (|| {
            self.episode
                .validate_bound(false, Some(&self.bound_files))?;
            let chosen = self
                .cursors
                .iter_mut()
                .filter(|cursor| cursor.position < cursor.end)
                .min_by_key(|cursor| {
                    let record = self.records[cursor.position];
                    (record.log_time, record.identity.order())
                });
            if let Some(cursor) = chosen {
                let record = self.records[cursor.position];
                cursor.position += 1;
                Ok(Some(record))
            } else {
                self.episode
                    .validate_bound(false, Some(&self.bound_files))?;
                self.ended = true;
                Ok(None)
            }
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn rewind(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(error(
                ErrorKind::InvalidSource,
                "source cursor previously failed",
                "terminal",
            ));
        }
        self.episode
            .validate_bound(false, Some(&self.bound_files))?;
        for cursor in &mut self.cursors {
            cursor.position = cursor.start;
        }
        self.ended = false;
        Ok(())
    }
}

// Grow in batches within the index budget.
fn push_record(
    records: &mut Vec<SourceRecord>,
    record: SourceRecord,
    bytes: usize,
) -> Result<(), Error> {
    if records.len() == records.capacity() {
        let left = bytes / size_of::<SourceRecord>() - records.len();
        if left == 0 {
            return Err(error(
                ErrorKind::ResourceLimit,
                "compact metadata index memory exhausted",
                bytes,
            ));
        }
        records
            .try_reserve_exact(left.min(1024))
            .map_err(|err| error(ErrorKind::ResourceLimit, "metadata index allocation", err))?;
    }
    records.push(record);
    Ok(())
}
