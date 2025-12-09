//! MCAP writer

use std::fs::File;
use std::io::{BufWriter, Seek};
use std::path::Path;
use std::sync::{Arc, Weak};
use std::{fmt::Debug, io::Write};

use crate::library_version::get_library_version;
use crate::sink_channel_filter::SinkChannelFilterFn;
use crate::{ChannelDescriptor, Context, FoxgloveError, Sink, SinkChannelFilter, SinkId};

/// Compression options for content in an MCAP file
pub use mcap::Compression as McapCompression;
/// Options for use with an [`McapWriter`][crate::McapWriter].
pub use mcap::WriteOptions as McapWriteOptions;

mod mcap_sink;
mod nonblocking_mcap_sink;

use mcap_sink::McapSink;
use nonblocking_mcap_sink::NonblockingMcapSink;

/// An MCAP writer for logging events.
///
/// ### Methods
///
/// - [`create`](McapWriter::create) - Synchronous. Writes to file directly, may block if involving disk I/O.
/// - [`create_nonblocking`](McapWriter::create_nonblocking) - Nonblocking. Queues writes to a background thread, never blocks.
/// - [`create_new_buffered_file`](McapWriter::create_new_buffered_file) - Synchronous.
///   Writes to a BufWriter, may block on disk I/O.
///
/// When the handle is dropped, buffered writes are flushed and the file is closed.
#[must_use]
#[derive(Clone)]
pub struct McapWriter {
    options: McapWriteOptions,
    context: Arc<Context>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
}

impl Debug for McapWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McapWriter")
            .field("options", &self.options)
            .field("context", &self.context)
            .finish_non_exhaustive()
    }
}

impl From<McapWriteOptions> for McapWriter {
    fn from(value: McapWriteOptions) -> Self {
        let options = value.library(get_library_version());
        Self {
            options,
            context: Context::get_default(),
            channel_filter: None,
        }
    }
}

impl Default for McapWriter {
    fn default() -> Self {
        Self::from(McapWriteOptions::default())
    }
}

impl McapWriter {
    /// Instantiates a new MCAP writer with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Instantiates a new MCAP writer with the provided options.
    /// The library option is ignored.
    pub fn with_options(options: McapWriteOptions) -> Self {
        options.into()
    }

    /// Sets the context for this sink.
    #[doc(hidden)]
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = ctx.clone();
        self
    }

    /// Sets a [`SinkChannelFilter`] for this file.
    pub fn channel_filter(mut self, filter: Arc<dyn SinkChannelFilter>) -> Self {
        self.channel_filter = Some(filter);
        self
    }

    /// Sets a channel filter for this file. See [`SinkChannelFilter`] for more information.
    pub fn channel_filter_fn(
        mut self,
        filter: impl Fn(&ChannelDescriptor) -> bool + Sync + Send + 'static,
    ) -> Self {
        self.channel_filter = Some(Arc::new(SinkChannelFilterFn(filter)));
        self
    }

    /// Begins logging events to the specified writer (synchronous).
    ///
    /// Each `log()` call writes directly to the file, which may block if the disk is slow.
    ///
    /// For high-throughput or real-time applications, consider using [`McapWriter::create_nonblocking`]
    /// instead, which queues writes and processes them in a background thread.
    ///
    /// Returns a handle. When the handle is dropped, the recording will be flushed to the writer
    /// and closed. Alternatively, the caller may choose to call [`McapWriterHandle::close`] to
    /// manually flush the recording and recover the writer.
    pub fn create<W>(self, writer: W) -> Result<McapWriterHandle<W>, FoxgloveError>
    where
        W: Write + Seek + Send + 'static,
    {
        let mcap_sink = Arc::new(McapSink::new(writer, self.options, self.channel_filter)?);
        self.context.add_sink(mcap_sink.clone()); // Register directly, no SinkKind wrapper
        Ok(McapWriterHandle {
            sink: SinkKind::Sync(mcap_sink),
            context: Arc::downgrade(&self.context),
        })
    }

    /// Begins logging events to the specified writer (nonblocking).
    ///
    /// Messages are queued and written by a background thread.
    /// The `log()` method returns immediately without blocking on disk I/O.
    ///
    /// Uses the default queue capacity (1024). For custom capacity, use
    /// [`create_nonblocking_with_capacity`](Self::create_nonblocking_with_capacity).
    ///
    /// **Note:** If the queue fills up, new messages are dropped silently.
    pub fn create_nonblocking<W>(self, writer: W) -> Result<McapWriterHandle<W>, FoxgloveError>
    where
        W: Write + Seek + Send + 'static,
    {
        self.create_nonblocking_with_capacity(writer, None)
    }

    /// Begins logging events to the specified writer (nonblocking) with custom queue capacity.
    ///
    /// Messages are queued and written by a background thread.
    /// The `log()` method returns immediately without blocking on disk I/O.
    ///
    /// # Arguments
    /// * `writer` - The underlying writer to write MCAP data to
    /// * `queue_capacity` - Max messages to buffer before dropping. `None` uses default (1024).
    ///
    /// **Note:** If the queue fills up, new messages are dropped silently.
    pub fn create_nonblocking_with_capacity<W>(
        self,
        writer: W,
        queue_capacity: Option<usize>,
    ) -> Result<McapWriterHandle<W>, FoxgloveError>
    where
        W: Write + Seek + Send + 'static,
    {
        let nonblocking_sink = Arc::new(NonblockingMcapSink::new(
            writer,
            self.options,
            self.channel_filter,
            queue_capacity,
        )?);
        self.context.add_sink(nonblocking_sink.clone()); // Register directly, no SinkKind wrapper
        Ok(McapWriterHandle {
            sink: SinkKind::Nonblocking(nonblocking_sink),
            context: Arc::downgrade(&self.context),
        })
    }

    /// Creates a new write-only buffered file, and begins logging events to it.
    ///
    /// If the file already exists, this call will fail with
    /// [`AlreadyExists`](`std::io::ErrorKind::AlreadyExists`).
    ///
    /// If you want more control over how the file is opened, or you want to write to something
    /// other than a file, use [`McapWriter::create`].
    pub fn create_new_buffered_file<P>(
        self,
        path: P,
    ) -> Result<McapWriterHandle<BufWriter<File>>, FoxgloveError>
    where
        P: AsRef<Path>,
    {
        let file = File::create_new(path)?;
        let writer = BufWriter::new(file);
        self.create(writer)
    }
}

/// The kind of sink (sync or nonblocking) backing the writer handle.
///
/// Note: The inner sinks are registered directly with Context (not wrapped in SinkKind),
/// so Context.log() dispatches directly to the sink without an extra enum match.
#[derive(Debug)]
enum SinkKind<W: Write + Seek + Send + 'static> {
    Sync(Arc<McapSink<W>>),
    Nonblocking(Arc<NonblockingMcapSink<W>>),
}

impl<W: Write + Seek + Send + 'static> SinkKind<W> {
    fn id(&self) -> SinkId {
        match self {
            SinkKind::Sync(sink) => sink.id(),
            SinkKind::Nonblocking(sink) => sink.id(),
        }
    }

    fn finish(&self) -> Result<W, FoxgloveError> {
        match self {
            SinkKind::Sync(sink) => sink.finish().map(|w| w.expect("not finished")),
            SinkKind::Nonblocking(sink) => sink.finish(),
        }
    }

    fn write_metadata(
        &self,
        name: &str,
        metadata: std::collections::BTreeMap<String, String>,
    ) -> Result<(), FoxgloveError> {
        match self {
            SinkKind::Sync(sink) => sink.write_metadata(name, metadata),
            SinkKind::Nonblocking(sink) => sink.write_metadata(name, metadata),
        }
    }
}

/// A handle to an MCAP file writer.
///
/// When dropped, it will unregister from the [`Context`], stop logging
/// events, and flush any buffered data to the writer.
#[must_use]
#[derive(Debug)]
pub struct McapWriterHandle<W: Write + Seek + Send + 'static> {
    sink: SinkKind<W>,
    context: Weak<Context>,
}

impl<W: Write + Seek + Send + 'static> McapWriterHandle<W> {
    /// Stops logging events, flushes buffered data, and returns the writer.
    ///
    /// This method blocks until all writes are complete (for both sync and nonblocking writers).
    pub fn close(self) -> Result<W, FoxgloveError> {
        self.remove_from_context();
        self.sink.finish()
    }

    /// Writes MCAP metadata to the file.
    ///
    /// If the metadata map is empty, this method returns early without writing anything.
    ///
    /// # Arguments
    /// * `name` - Name identifier for this metadata record
    /// * `metadata` - Key-value pairs to store (empty map will be skipped)
    ///
    pub fn write_metadata(
        &self,
        name: &str,
        metadata: std::collections::BTreeMap<String, String>,
    ) -> Result<(), FoxgloveError> {
        self.sink.write_metadata(name, metadata)
    }

    /// Removes this sink from the context (if context still exists).
    fn remove_from_context(&self) {
        if let Some(context) = self.context.upgrade() {
            context.remove_sink(self.sink.id());
        }
    }
}

impl<W: Write + Seek + Send + 'static> Drop for McapWriterHandle<W> {
    fn drop(&mut self) {
        self.remove_from_context();
    }
}
