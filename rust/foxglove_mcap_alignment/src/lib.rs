//! Experimental internal alignment selection. This crate makes no public SDK promise.
//!
//! The selector consumes immutable ordered metadata. `local_source` validates local
//! MCAP snapshots and serves ordered metadata from a bounded in-memory index.
//! `executor` materializes independent messages with bounded chunk access and atomic
//! single-artifact publication. `media` isolates codec implementations behind a
//! private boundary while retaining source identity and resource accounting here.

use std::collections::HashSet;
use std::fmt;

pub mod executor;
pub mod local_source;
pub mod media;
mod targets;
pub use targets::{Rate, Targets, Timeline};

/// A category from version 1 of the experimental alignment contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidConfiguration,
    InvalidSource,
    SourceChanged,
    UnsupportedRecord,
    DuplicateReference,
    EmptyTimeline,
    MissingRequired,
    ArithmeticOverflow,
    ResourceLimit,
    UnsupportedMedia,
    DecodeFailed,
    EncodeFailed,
    UnsupportedMissingPolicy,
    Cancelled,
    IoFailed,
    ValidationFailed,
}

/// Selection failure, with target/stream context when a required slot is absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: &'static str,
    pub detail: Option<String>,
    pub target: Option<(u64, u64)>,
    pub stream: Option<usize>,
}

impl Error {
    fn new(kind: ErrorKind, message: &'static str) -> Self {
        Self {
            kind,
            message,
            detail: None,
            target: None,
            stream: None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}: {} (target={:?}, stream={:?})",
            self.kind, self.message, self.target, self.stream
        )?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}
impl std::error::Error for Error {}

/// Validated half-open episode bounds, both in log-time nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Episode {
    start: u64,
    end: u64,
}

impl Episode {
    pub fn new(start: u64, end: u64) -> Result<Self, Error> {
        if start >= end {
            return Err(Error::new(
                ErrorKind::InvalidConfiguration,
                "episode must have start < end",
            ));
        }
        Ok(Self { start, end })
    }

    pub fn start(self) -> u64 {
        self.start
    }
    pub fn end(self) -> u64 {
        self.end
    }
}

/// Complete-byte file version. Its position in the inventory is its configured ordinal.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct FileIdentity {
    pub sha256: [u8; 32],
    pub byte_length: u64,
}

/// The physical Message opcode location, independent of timestamps and sequence.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum Locator {
    Unchunked {
        offset: u64,
    },
    Chunked {
        chunk_offset: u64,
        message_offset: u64,
    },
}

impl Locator {
    fn order(self) -> (u64, u64) {
        match self {
            Self::Unchunked { offset } => (offset, 0),
            Self::Chunked {
                chunk_offset,
                message_offset,
            } => (chunk_offset, message_offset),
        }
    }
}

/// File/channel identity bound by the source adapter to its schema/serialization.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct SourceIdentity {
    pub file: usize,
    pub channel: u16,
    pub locator: Locator,
}

impl SourceIdentity {
    fn order(self) -> (usize, (u64, u64), u16) {
        (self.file, self.locator.order(), self.channel)
    }
}

/// Immutable source-reference metadata; timestamps and sequence are not identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRecord {
    pub identity: SourceIdentity,
    pub log_time: u64,
    pub publish_time: u64,
    pub sequence: u32,
}

/// An ordered logical stream, borrowing adapter-owned metadata without copying it.
#[derive(Clone, Copy, Debug)]
pub struct Records<'a> {
    records: &'a [SourceRecord],
}

impl<'a> Records<'a> {
    /// Validate inventory uniqueness, physical identities, file bounds and stable order.
    ///
    /// The caller resolves stream bindings/schema compatibility before constructing this
    /// view. Ordering is by time, configured file, physical locator, then channel.
    /// Validation uses transient O(records) identity storage; it builds no selection matrix.
    pub fn new(records: &'a [SourceRecord], files: &[FileIdentity]) -> Result<Self, Error> {
        if files.iter().collect::<HashSet<_>>().len() != files.len() {
            return Err(Error::new(
                ErrorKind::InvalidConfiguration,
                "file version configured more than once",
            ));
        }
        let mut identities = HashSet::new();
        identities
            .try_reserve(records.len())
            .map_err(|_| Error::new(ErrorKind::ResourceLimit, "identity validation allocation"))?;
        for record in records {
            let Some(file) = files.get(record.identity.file) else {
                return Err(Error::new(ErrorKind::InvalidSource, "unknown file ordinal"));
            };
            let offset = record.identity.locator.order().0;
            if offset >= file.byte_length {
                return Err(Error::new(
                    ErrorKind::InvalidSource,
                    "record/chunk offset outside file",
                ));
            }
            if !identities.insert(record.identity) {
                return Err(Error::new(
                    ErrorKind::InvalidSource,
                    "repeated physical record identity",
                ));
            }
        }
        if records.windows(2).any(|pair| {
            (pair[0].log_time, pair[0].identity.order())
                > (pair[1].log_time, pair[1].identity.order())
        }) {
            return Err(Error::new(
                ErrorKind::InvalidSource,
                "records must be in stable timestamp order",
            ));
        }
        Ok(Self { records })
    }
}

/// Per-stream matching policy, with finite unsigned tolerance in nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    Previous,
    Nearest,
    Exact,
}

/// Resolved sampled-stream configuration. Optional misses remain diagnostic slots.
pub struct Stream<'a> {
    pub source: Box<dyn MetadataSource + 'a>,
    pub policy: Policy,
    pub tolerance_ns: u64,
    pub required: bool,
}

/// A valid reference contains the immutable source record and signed source-target offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub source: Option<SourceRecord>,
    pub offset_ns: Option<i128>,
    pub valid: bool,
    pub reused: bool,
}

/// One target's slots, always in configured stream order.
#[derive(Debug, PartialEq, Eq)]
pub struct TargetSelection {
    pub index: u64,
    pub time: u64,
    pub slots: Vec<Selection>,
}

/// Fallible ordered metadata input. The selector consumes one record at a time.
///
/// The source adapter validates file versions, channel bindings and globally unique
/// physical identities. The selector validates stable ordering incrementally.
pub trait MetadataSource {
    fn next_record(&mut self) -> Result<Option<SourceRecord>, Error>;

    /// Restart the same immutable metadata traversal; reference timelines validate
    /// duplicates before sampling. File-backed implementations must preserve their
    /// validated source snapshot across rewinds.
    fn rewind(&mut self) -> Result<(), Error>;
}

/// In-memory metadata source, borrowing a validated slice for tests/small indexes.
pub struct SliceSource<'a> {
    records: Records<'a>,
    next: usize,
}

impl<'a> SliceSource<'a> {
    pub fn new(records: Records<'a>) -> Self {
        Self { records, next: 0 }
    }
}
impl MetadataSource for SliceSource<'_> {
    fn rewind(&mut self) -> Result<(), Error> {
        self.next = 0;
        Ok(())
    }
    fn next_record(&mut self) -> Result<Option<SourceRecord>, Error> {
        let record = self.records.records.get(self.next).copied();
        if record.is_some() {
            self.next += 1;
        }
        Ok(record)
    }
}

struct OrderedSource<'a> {
    source: Box<dyn MetadataSource + 'a>,
    last: Option<SourceRecord>,
    episode: Episode,
    ended: bool,
}
impl<'a> OrderedSource<'a> {
    fn new(source: Box<dyn MetadataSource + 'a>, episode: Episode) -> Self {
        Self {
            source,
            last: None,
            episode,
            ended: false,
        }
    }
    fn next_record(&mut self) -> Result<Option<SourceRecord>, Error> {
        if self.ended {
            return Ok(None);
        }
        while let Some(record) = self.source.next_record()? {
            if self.last.is_some_and(|last| {
                (last.log_time, last.identity.order()) >= (record.log_time, record.identity.order())
            }) {
                return Err(Error::new(
                    ErrorKind::InvalidSource,
                    "records must have unique stable timestamp ordering",
                ));
            }
            self.last = Some(record);
            if record.log_time >= self.episode.end {
                self.ended = true;
                return Ok(None);
            }
            if record.log_time >= self.episode.start {
                return Ok(Some(record));
            }
        }
        self.ended = true;
        Ok(None)
    }
}

struct Cursor<'a> {
    source: OrderedSource<'a>,
    next: Option<SourceRecord>,
    previous: Option<SourceRecord>,
    last_selected: Option<SourceIdentity>,
    policy: Policy,
    tolerance: u64,
    required: bool,
    advanced: usize,
}

impl Cursor<'_> {
    fn select(&mut self, target: u64) -> Result<Selection, Error> {
        while self.next.is_some_and(|record| record.log_time < target) {
            let group = self.next;
            while self
                .next
                .is_some_and(|record| Some(record.log_time) == group.map(|first| first.log_time))
            {
                self.next = self.source.next_record()?;
                self.advanced += 1;
            }
            self.previous = group;
        }
        let after = self.next;
        let before = self.previous;
        let candidate = match self.policy {
            Policy::Exact => after.filter(|record| record.log_time == target),
            Policy::Previous => after.filter(|record| record.log_time == target).or(before),
            Policy::Nearest => match (before, after) {
                (Some(left), Some(right)) if target - left.log_time <= right.log_time - target => {
                    Some(left)
                }
                (_, Some(right)) => Some(right),
                (left, None) => left,
            },
        };
        let source = candidate.filter(|record| record.log_time.abs_diff(target) <= self.tolerance);
        let identity = source.map(|record| record.identity);
        let reused = identity.is_some() && identity == self.last_selected;
        self.last_selected = identity;
        Ok(Selection {
            source,
            offset_ns: source.map(|record| i128::from(record.log_time) - i128::from(target)),
            valid: source.is_some(),
            reused,
        })
    }
}

/// Bounded sequential result generation, retaining only one cursor/reuse state per stream.
///
/// A required miss is terminal and discards the current batch. Earlier returned batches
/// are diagnostic references only; a later executor must not publish them prematurely.
/// Each stream retains two source records. Sources control their own index/cache budget.
pub struct Selector<'a> {
    targets: Targets<'a>,
    cursors: Vec<Cursor<'a>>,
    batch_targets: usize,
    index: u64,
    failed: bool,
}

impl<'a> Selector<'a> {
    pub fn new(
        episode: Episode,
        timeline: Timeline<'a>,
        streams: Vec<Stream<'a>>,
        batch_targets: usize,
    ) -> Result<Self, Error> {
        if batch_targets == 0 {
            return Err(Error::new(
                ErrorKind::InvalidConfiguration,
                "batch target limit must be positive",
            ));
        }
        let mut cursors = Vec::new();
        cursors
            .try_reserve(streams.len())
            .map_err(|_| Error::new(ErrorKind::ResourceLimit, "cursor allocation"))?;
        for stream in streams {
            if stream.policy == Policy::Exact && stream.tolerance_ns != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidConfiguration,
                    "exact selection requires zero tolerance",
                ));
            }
            let mut source = OrderedSource::new(stream.source, episode);
            let next = source.next_record()?;
            cursors.push(Cursor {
                source,
                next,
                previous: None,
                last_selected: None,
                policy: stream.policy,
                tolerance: stream.tolerance_ns,
                required: stream.required,
                advanced: 0,
            });
        }
        Ok(Self {
            targets: Targets::new(episode, timeline)?,
            cursors,
            batch_targets,
            index: 0,
            failed: false,
        })
    }

    pub fn next_batch(&mut self) -> Result<Option<Vec<TargetSelection>>, Error> {
        if self.failed {
            return Ok(None);
        }
        let result = self.generate_batch();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn generate_batch(&mut self) -> Result<Option<Vec<TargetSelection>>, Error> {
        let mut batch = Vec::new();
        // Reserve incrementally: a caller's huge limit need not allocate past the timeline.
        for _ in 0..self.batch_targets {
            let Some(time) = self.targets.next_time()? else {
                break;
            };
            batch
                .try_reserve(1)
                .map_err(|_| Error::new(ErrorKind::ResourceLimit, "target batch allocation"))?;
            let mut slots = Vec::new();
            slots
                .try_reserve(self.cursors.len())
                .map_err(|_| Error::new(ErrorKind::ResourceLimit, "slot allocation"))?;
            for (stream, cursor) in self.cursors.iter_mut().enumerate() {
                let slot = cursor.select(time)?;
                if cursor.required && !slot.valid {
                    return Err(Error {
                        kind: ErrorKind::MissingRequired,
                        message: "no source within policy tolerance",
                        detail: None,
                        target: Some((self.index, time)),
                        stream: Some(stream),
                    });
                }
                slots.push(slot);
            }
            batch.push(TargetSelection {
                index: self.index,
                time,
                slots,
            });
            self.index = self.index.checked_add(1).ok_or_else(|| {
                Error::new(ErrorKind::ArithmeticOverflow, "target index overflow")
            })?;
        }
        Ok((!batch.is_empty()).then_some(batch))
    }

    /// Number of input records traversed by each selection cursor (excluding validation).
    /// This permits checking monotonic traversal independently of noisy wall-clock timing.
    pub fn records_advanced(&self) -> impl Iterator<Item = usize> + '_ {
        self.cursors.iter().map(|cursor| cursor.advanced)
    }
}
