//! In-memory physical selection tables for the compact alignment example.

use std::sync::Arc;

use crate::local_source::{Binding, EpisodeFiles, LocalSources, error, push_record};
use crate::{Error, ErrorKind, MetadataSource, SourceRecord};

struct SortLease {
    episode: Arc<EpisodeFiles>,
    bytes: usize,
}

impl Drop for SortLease {
    fn drop(&mut self) {
        if let Ok(mut used) = self.episode.sort_memory.lock() {
            *used -= self.bytes;
        }
    }
}

pub(crate) struct PhysicalRecords<'a> {
    _sources: &'a LocalSources,
    lease: SortLease,
    records: Vec<SourceRecord>,
}

impl LocalSources {
    pub(crate) fn selected_physical_records(&self) -> Result<PhysicalRecords<'_>, Error> {
        let bytes = self.limits.index_memory_bytes / 4;
        let mut used = self
            .episode
            .sort_memory
            .lock()
            .map_err(|e| error(ErrorKind::InvalidSource, "sort memory lock poisoned", e))?;
        if *used > self.limits.index_memory_bytes - bytes {
            return Err(error(
                ErrorKind::ResourceLimit,
                "physical sort budget exhausted",
                bytes,
            ));
        }
        *used += bytes;
        Ok(PhysicalRecords {
            _sources: self,
            lease: SortLease {
                episode: Arc::clone(&self.episode),
                bytes,
            },
            records: Vec::new(),
        })
    }

    pub(crate) fn physical_source(&self, bindings: &[Binding]) -> Result<PhysicalCursor, Error> {
        let mut selected = self.selected_physical_records()?;
        let mut input = self.metadata_source(bindings)?;
        while let Some(record) = input.next_record()? {
            selected.push(record)?;
        }
        selected.finish_cursor()
    }
}

impl PhysicalRecords<'_> {
    pub(crate) fn push(&mut self, record: SourceRecord) -> Result<(), Error> {
        push_record(&mut self.records, record, self.lease.bytes)
    }

    pub(crate) fn finish_cursor(mut self) -> Result<PhysicalCursor, Error> {
        self.records
            .sort_unstable_by_key(|record| record.identity.order());
        if self
            .records
            .windows(2)
            .any(|pair| pair[0].identity == pair[1].identity && pair[0] != pair[1])
        {
            return Err(Error::new(
                ErrorKind::InvalidSource,
                "conflicting physical references",
            ));
        }
        self.records.dedup();
        let retained = self.records.capacity() * size_of::<SourceRecord>();
        let mut used = self
            .lease
            .episode
            .sort_memory
            .lock()
            .map_err(|e| error(ErrorKind::InvalidSource, "sort memory lock poisoned", e))?;
        *used -= self.lease.bytes - retained;
        drop(used);
        self.lease.bytes = retained;
        Ok(PhysicalCursor {
            table: Arc::new(PhysicalTable {
                _lease: self.lease,
                records: self.records,
            }),
            position: 0,
        })
    }

    pub(crate) fn finish(self) -> Result<u64, Error> {
        Ok(self.finish_cursor()?.count())
    }
}

struct PhysicalTable {
    _lease: SortLease,
    records: Vec<SourceRecord>,
}

pub(crate) struct PhysicalCursor {
    table: Arc<PhysicalTable>,
    position: usize,
}

impl PhysicalCursor {
    pub(crate) fn count(&self) -> u64 {
        self.table.records.len() as u64
    }
    pub(crate) fn fork(&self) -> Result<Self, Error> {
        Ok(Self {
            table: Arc::clone(&self.table),
            position: 0,
        })
    }
    pub(crate) fn next_record(&mut self) -> Result<Option<SourceRecord>, Error> {
        self.table._lease.episode.validate(false)?;
        let record = self.table.records.get(self.position).copied();
        self.position += usize::from(record.is_some());
        Ok(record)
    }
}
