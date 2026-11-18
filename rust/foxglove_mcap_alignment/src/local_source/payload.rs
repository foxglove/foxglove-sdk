//! Payload access and writer reservations share the immutable episode owner.
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mcap::records::Record;

use crate::local_source::{EpisodeFiles, FileInventory, LocalSources, error, io_error, wire};
use crate::{Error, ErrorKind, FileIdentity, Locator, SourceRecord};

pub(crate) struct ChargedFile {
    file: File,
    episode: Arc<EpisodeFiles>,
    length: u64,
    cap: u64,
    filesystem: PathBuf,
    next_free_check: u64,
}
impl ChargedFile {
    pub(crate) fn sync(&self) -> Result<(), Error> {
        self.file.sync_all().map_err(io_error)
    }
}
impl Drop for ChargedFile {
    fn drop(&mut self) {
        if let Ok(mut storage) = self.episode.storage.lock() {
            storage.used -= self.length;
        }
    }
}
impl Seek for ChargedFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let result = self.file.seek(pos);
        self.record_failure(result)
    }
}
impl ChargedFile {
    fn write_bounded(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .file
            .stream_position()?
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| {
                std::io::Error::other(Error::new(ErrorKind::ResourceLimit, "output size overflow"))
            })?;
        if end > self.cap {
            return Err(std::io::Error::other(Error::new(
                ErrorKind::ResourceLimit,
                "output byte budget exhausted",
            )));
        }
        if end > self.length {
            let growth = end - self.length;
            if end > self.next_free_check {
                let available = fs2::available_space(&self.filesystem)?;
                if growth > available {
                    return Err(std::io::Error::other(Error::new(
                        ErrorKind::ResourceLimit,
                        "output filesystem free space exhausted",
                    )));
                }
                self.next_free_check = end.saturating_add(1024 * 1024);
            }
            self.episode
                .storage
                .lock()
                .map_err(|_| std::io::Error::other("storage lock poisoned"))?
                .reserve_bytes(growth, false)
                .map_err(std::io::Error::other)?;
            self.length = end;
        }
        self.file.write(bytes)
    }
}
impl ChargedFile {
    fn record_failure<T>(&self, result: std::io::Result<T>) -> std::io::Result<T> {
        if let Err(err) = &result {
            let failure = err
                .get_ref()
                .and_then(|inner| inner.downcast_ref::<Error>())
                .cloned()
                .unwrap_or_else(|| {
                    error(
                        if matches!(err.raw_os_error(), Some(23 | 24 | 28)) {
                            ErrorKind::ResourceLimit
                        } else {
                            ErrorKind::IoFailed
                        },
                        "staging write failed",
                        err,
                    )
                });
            if let Ok(mut latch) = self.episode.write_failure.lock() {
                latch.get_or_insert(failure);
            }
        }
        result
    }
}
impl Write for ChargedFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let result = self.write_bounded(bytes);
        self.record_failure(result)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let result = self.file.flush();
        self.record_failure(result)
    }
}

pub(crate) struct PayloadReader<'a> {
    sources: &'a LocalSources,
    cache: Option<((usize, u64), Vec<u8>)>,
    cache_cap: usize,
    pub(crate) read_bytes: u64,
    pub(crate) decompressions: u64,
    pub(crate) cache_hits: u64,
}
impl<'a> PayloadReader<'a> {
    pub(crate) fn new(sources: &'a LocalSources, cache_cap: usize) -> Self {
        Self {
            sources,
            cache: None,
            cache_cap,
            read_bytes: 0,
            decompressions: 0,
            cache_hits: 0,
        }
    }
    pub(crate) fn outer(&mut self, file: usize, offset: u64) -> Result<(u8, Vec<u8>), Error> {
        let inventory =
            self.sources.files().get(file).ok_or_else(|| {
                error(ErrorKind::InvalidConfiguration, "unknown source file", file)
            })?;
        let pinned = self
            .sources
            .episode
            .pinned
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "source lock poisoned", file))?;
        let mut input = File::open(&pinned[file].snapshot).map_err(io_error)?;
        let (opcode, bytes, next) = wire::frame(
            &mut input,
            offset,
            inventory.identity.byte_length,
            &self.sources.limits,
        )?;
        self.read_bytes += next - offset;
        Ok((opcode, bytes))
    }
    pub(crate) fn message(&mut self, record: SourceRecord) -> Result<Vec<u8>, Error> {
        let (opcode, body) = match record.identity.locator {
            Locator::Unchunked { offset } => self.outer(record.identity.file, offset)?,
            Locator::Chunked {
                chunk_offset,
                message_offset,
            } => {
                let key = (record.identity.file, chunk_offset);
                let bytes = if self
                    .cache
                    .as_ref()
                    .is_some_and(|(cached, _)| *cached == key)
                {
                    self.cache_hits += 1;
                    &self.cache.as_ref().expect("cache key checked").1
                } else {
                    self.cache = None;
                    let (opcode, body) = self.outer(key.0, key.1)?;
                    let Record::Chunk { header, data } = wire::parse(opcode, &body, key.1)? else {
                        return Err(error(
                            ErrorKind::InvalidSource,
                            "locator does not reference chunk",
                            key.1,
                        ));
                    };
                    let decoded = wire::decompress(&header, &data, &self.sources.limits, key.1)?;
                    if !header.compression.is_empty() {
                        self.decompressions += 1;
                    }
                    // One requested chunk is the bounded fallback buffer when it exceeds cache.
                    self.cache = Some((key, decoded));
                    &self.cache.as_ref().expect("chunk installed").1
                };
                let offset = usize::try_from(message_offset)
                    .map_err(|e| error(ErrorKind::InvalidSource, "message offset overflow", e))?;
                let header = bytes.get(offset..offset.saturating_add(9)).ok_or_else(|| {
                    error(
                        ErrorKind::InvalidSource,
                        "message locator out of chunk",
                        offset,
                    )
                })?;
                let length = u64::from_le_bytes(header[1..].try_into().expect("header length"));
                let length = usize::try_from(length)
                    .map_err(|e| error(ErrorKind::InvalidSource, "message length overflow", e))?;
                let body = bytes
                    .get(offset + 9..)
                    .and_then(|rest| rest.get(..length))
                    .ok_or_else(|| {
                        error(
                            ErrorKind::InvalidSource,
                            "message extends beyond chunk",
                            offset,
                        )
                    })?;
                (header[0], body.to_vec())
            }
        };
        let Record::Message { header, data } =
            wire::parse(opcode, &body, record.identity.locator.order().0)?
        else {
            return Err(error(
                ErrorKind::InvalidSource,
                "locator does not reference message",
                opcode,
            ));
        };
        if header.channel_id != record.identity.channel
            || header.log_time != record.log_time
            || header.publish_time != record.publish_time
            || header.sequence != record.sequence
        {
            return Err(error(
                ErrorKind::InvalidSource,
                "source reference metadata mismatch",
                format!("{:?}", record.identity),
            ));
        }
        let result = data.into_owned();
        if self
            .cache
            .as_ref()
            .is_some_and(|(_, bytes)| bytes.len() > self.cache_cap)
        {
            self.cache = None;
        }
        Ok(result)
    }
}
impl LocalSources {
    pub(crate) fn write_failure_latch(&self) -> Arc<std::sync::Mutex<Option<Error>>> {
        Arc::clone(&self.episode.write_failure)
    }
    pub(crate) fn execution_handles(&self) -> Result<super::CursorLease, Error> {
        self.reserve_execution_handles(10)
    }
    pub(crate) fn reserve_execution_handles(
        &self,
        handles: usize,
    ) -> Result<super::CursorLease, Error> {
        let mut memory = self
            .episode
            .memory
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "handle lock poisoned", "executor"))?;
        let next = memory
            .handles
            .checked_add(handles)
            .ok_or_else(|| Error::new(ErrorKind::ResourceLimit, "executor handle overflow"))?;
        if next > memory.handle_limit {
            return Err(error(
                ErrorKind::ResourceLimit,
                "executor file-handle reservation exhausted",
                format!("required {next}, limit {}", memory.handle_limit),
            ));
        }
        memory.handles = next;
        Ok(super::CursorLease {
            episode: Arc::clone(&self.episode),
            bytes: 0,
            handles,
        })
    }
    pub(crate) fn execution_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, Error> {
        self.episode.execution.try_lock().map_err(|e| {
            error(
                ErrorKind::ResourceLimit,
                "one executor per source episode",
                e,
            )
        })
    }
    pub(crate) fn charged_output(&self, file: File, cap: u64, filesystem: &Path) -> ChargedFile {
        ChargedFile {
            file,
            episode: Arc::clone(&self.episode),
            length: 0,
            cap,
            filesystem: filesystem.to_path_buf(),
            next_free_check: 0,
        }
    }
    pub(crate) fn storage_peak(&self) -> Result<u64, Error> {
        Ok(self
            .episode
            .storage
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "storage lock poisoned", "peak"))?
            .peak)
    }
    pub(crate) fn validate_output(&self, path: &Path, writer_memory: usize) -> Result<(), Error> {
        let mut inventory = FileInventory {
            path: PathBuf::from(path),
            identity: FileIdentity {
                sha256: [0; 32],
                byte_length: std::fs::metadata(path).map_err(io_error)?.len(),
            },
            channels: Default::default(),
            schemas: Default::default(),
        };
        let mut limits = self.limits;
        limits.max_record_bytes = limits.max_record_bytes.max(1024 * 1024);
        limits.max_chunk_bytes = limits.max_chunk_bytes.max(
            limits
                .max_decompressed_chunk_bytes
                .saturating_add(2 * 1024 * 1024),
        );
        limits.inventory_memory_bytes = writer_memory;
        limits.max_decompressed_chunk_bytes = limits
            .max_decompressed_chunk_bytes
            .saturating_add(2 * 1024 * 1024);
        wire::scan(path, &mut inventory, &limits, None, &mut 0)?;
        Ok(())
    }
}
impl std::io::Read for ChargedFile {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(bytes)
    }
}
impl LocalSources {
    pub(crate) fn verify_message(&self, record: SourceRecord) -> Result<(), Error> {
        use crate::MetadataSource;
        let mut cursor = self.metadata_source(&[crate::local_source::Binding {
            file: record.identity.file,
            channel: record.identity.channel,
        }])?;
        while let Some(actual) = cursor.next_record()? {
            if actual.identity == record.identity {
                return if actual == record {
                    Ok(())
                } else {
                    Err(error(
                        ErrorKind::InvalidSource,
                        "context source metadata mismatch",
                        format!("{:?}", record.identity),
                    ))
                };
            }
        }
        Err(error(
            ErrorKind::InvalidSource,
            "context locator not in validated message index",
            format!("{:?}", record.identity),
        ))
    }
    pub(crate) fn verify_outer(&self, file: usize, wanted: u64) -> Result<(), Error> {
        let inventory = self.files().get(file).ok_or_else(|| {
            error(
                ErrorKind::InvalidConfiguration,
                "unknown context file",
                file,
            )
        })?;
        let pinned = self
            .episode
            .pinned
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "source lock poisoned", file))?;
        let mut input = File::open(&pinned[file].snapshot).map_err(io_error)?;
        let mut offset = 8;
        while offset < wanted {
            let (_, _, next) = wire::frame(
                &mut input,
                offset,
                inventory.identity.byte_length,
                &self.limits,
            )?;
            offset = next;
        }
        if offset == wanted {
            Ok(())
        } else {
            Err(error(
                ErrorKind::InvalidSource,
                "context offset is not a physical record boundary",
                wanted,
            ))
        }
    }
    pub(crate) fn bound_count(&self, bindings: &[crate::local_source::Binding]) -> u64 {
        bindings
            .iter()
            .map(|binding| self.ranges.get(binding).map_or(0, |range| range.1 as u64))
            .sum()
    }
    pub(crate) fn parse_outer<'a>(
        &self,
        opcode: u8,
        bytes: &'a [u8],
        offset: u64,
    ) -> Result<Record<'a>, Error> {
        wire::parse(opcode, bytes, offset)
    }
}
impl LocalSources {
    pub(crate) fn selected_records(
        &self,
    ) -> Result<crate::local_source::PhysicalRecords<'_>, Error> {
        self.selected_physical_records()
    }
    pub(crate) fn scratch_guard(&self) -> Result<ScratchGuard<'_>, Error> {
        let storage = self
            .episode
            .storage
            .lock()
            .map_err(|_| error(ErrorKind::InvalidSource, "storage lock poisoned", "scratch"))?;
        Ok(ScratchGuard {
            sources: self,
            serial: storage.serial,
            used: storage.used,
        })
    }
}
pub(crate) struct ScratchGuard<'a> {
    sources: &'a LocalSources,
    serial: u64,
    used: u64,
}
impl Drop for ScratchGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut storage) = self.sources.episode.storage.lock() {
            if let Ok(entries) = std::fs::read_dir(storage.directory.path()) {
                for entry in entries.flatten() {
                    if entry
                        .file_name()
                        .to_str()
                        .and_then(|s| s.parse::<u64>().ok())
                        .is_some_and(|serial| serial > self.serial)
                    {
                        let _result = std::fs::remove_file(entry.path());
                    }
                }
            }
            storage.used = self.used;
        }
    }
}
