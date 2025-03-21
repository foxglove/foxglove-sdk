//! MCAP writer

use std::fs::File;
use std::io::{BufWriter, Seek};
use std::path::Path;
use std::sync::{Arc, Weak};
use std::{fmt::Debug, io::Write};

use crate::{Context, FoxgloveError, Sink};
use mcap::WriteOptions;

mod mcap_sink;
use mcap_sink::McapSink;

/// An MCAP writer for logging events.
#[must_use]
#[derive(Debug, Clone)]
pub struct McapWriter {
    options: WriteOptions,
    context: Arc<Context>,
}

impl From<WriteOptions> for McapWriter {
    fn from(value: WriteOptions) -> Self {
        let options = value.library(format!("foxglove-sdk-rs-{}", env!("CARGO_PKG_VERSION")));
        Self {
            options,
            context: Context::get_default(),
        }
    }
}

impl Default for McapWriter {
    fn default() -> Self {
        Self::from(WriteOptions::default())
    }
}

impl McapWriter {
    /// Instantiates a new MCAP writer with default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Instantiates a new MCAP writer with the provided options.
    /// The library option is ignored.
    pub fn with_options(options: WriteOptions) -> Self {
        options.into()
    }

    /// Sets the context for this sink.
    #[doc(hidden)]
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = ctx.clone();
        self
    }

    /// Begins logging events to the specified writer.
    ///
    /// Returns a handle. When the handle is dropped, the recording will be flushed to the writer
    /// and closed. Alternatively, the caller may choose to call [`McapWriterHandle::close`] to
    /// manually flush the recording and recover the writer.
    pub fn create<W>(self, writer: W) -> Result<McapWriterHandle<W>, FoxgloveError>
    where
        W: Write + Seek + Send + 'static,
    {
        let sink = McapSink::new(writer, self.options)?;
        self.context.add_sink(sink.clone());
        Ok(McapWriterHandle {
            sink,
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

/// A handle to an MCAP file writer.
///
/// When this handle is dropped, the writer will stop logging events, and flush any buffered data
/// to the writer.
#[must_use]
#[derive(Debug)]
pub struct McapWriterHandle<W: Write + Seek + Send + 'static> {
    sink: Arc<McapSink<W>>,
    context: Weak<Context>,
}

impl<W: Write + Seek + Send + 'static> McapWriterHandle<W> {
    /// Stops logging events, flushes buffered data, and returns the writer.
    pub fn close(self) -> Result<W, FoxgloveError> {
        // It's safe to unwrap the `Option<W>` because `McapWriterHandle` doesn't implement clone,
        // and this method consumes self.
        self.finish().map(|w| w.expect("not finished"))
    }

    fn finish(&self) -> Result<Option<W>, FoxgloveError> {
        if let Some(context) = self.context.upgrade() {
            context.remove_sink(self.sink.id());
        }
        self.sink.finish()
    }
}

impl<W: Write + Seek + Send + 'static> Drop for McapWriterHandle<W> {
    fn drop(&mut self) {
        if let Err(e) = self.finish() {
            tracing::warn!("{e}");
        }
    }
}
