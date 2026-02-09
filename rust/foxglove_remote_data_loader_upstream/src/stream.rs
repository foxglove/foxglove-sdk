//! Internal MCAP streaming infrastructure.
//!
//! Provides [`Channel`], [`StreamHandle`], and [`McapStream`] for auto-flushing MCAP data.

use std::io::{Seek, SeekFrom, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use bytes::{Bytes, BytesMut};
use futures::{ready, Stream};
use parking_lot::Mutex;
use tokio::sync::mpsc::{Receiver, Sender};

use foxglove::{Context, Encode, McapWriteOptions, McapWriterHandle, ToUnixNanos};

use crate::BoxError;

// ---------------------------------------------------------------------------
// SharedBuffer
// ---------------------------------------------------------------------------

#[derive(Default)]
struct SharedBufferInner {
    buffer: BytesMut,
    position: u64,
}

#[derive(Default, Clone)]
struct SharedBuffer(Arc<Mutex<SharedBufferInner>>);

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut inner = self.0.lock();
        inner.buffer.extend_from_slice(buf);
        inner.position += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for SharedBuffer {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let inner = self.0.lock();
        match pos {
            SeekFrom::Start(n) if inner.position == n => Ok(n),
            SeekFrom::Current(0) => Ok(inner.position),
            _ => Err(std::io::Error::other("seek on unseekable stream")),
        }
    }
}

// ---------------------------------------------------------------------------
// StreamHandle
// ---------------------------------------------------------------------------

struct StreamHandleInner {
    buffer: SharedBuffer,
    sender: Sender<BytesMut>,
    writer: Mutex<Option<McapWriterHandle<SharedBuffer>>>,
    context: Arc<Context>,
    flush_threshold: usize,
}

/// Handle for an in-progress MCAP stream.
///
/// This is used internally by the framework to manage the MCAP writer and buffer.
/// Channels created from this handle will automatically flush the buffer when it
/// exceeds the configured threshold.
#[derive(Clone)]
pub(crate) struct StreamHandle(Arc<StreamHandleInner>);

impl StreamHandle {
    /// Try to flush if buffer exceeds threshold.
    fn try_flush(&self) {
        let mut guard = self.0.buffer.0.lock();
        if guard.buffer.len() >= self.0.flush_threshold {
            let bytes = guard.buffer.split();
            // Best effort - if channel full, data stays for next flush or close
            let _ = self.0.sender.try_send(bytes);
        }
    }

    /// Close the MCAP writer and flush remaining data.
    pub async fn close(&self) -> Result<(), BoxError> {
        if let Some(writer) = self.0.writer.lock().take() {
            writer.close()?;
        }
        // Final flush
        let bytes = self.0.buffer.0.lock().buffer.split();
        if !bytes.is_empty() {
            self.0
                .sender
                .send(bytes)
                .await
                .map_err(|_| "stream channel closed")?;
        }
        Ok(())
    }

    /// Blocking version of close for the blocking API.
    pub fn close_blocking(&self) -> Result<(), BoxError> {
        if let Some(writer) = self.0.writer.lock().take() {
            writer.close()?;
        }
        // Final flush (blocking)
        let bytes = self.0.buffer.0.lock().buffer.split();
        if !bytes.is_empty() {
            self.0
                .sender
                .blocking_send(bytes)
                .map_err(|_| "stream channel closed")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Channel
// ---------------------------------------------------------------------------

/// A channel for logging time-stamped messages.
///
/// Created via [`ChannelRegistry::channel`](crate::ChannelRegistry::channel) during
/// [`initialize`](crate::UpstreamServer::initialize). Store it in your
/// [`Context`](crate::UpstreamServer::Context) and use it in
/// [`stream`](crate::UpstreamServer::stream) to log messages.
///
/// Buffer management is automatic: the underlying MCAP buffer is flushed whenever
/// it exceeds a configurable threshold (see `FOXGLOVE_FLUSH_THRESHOLD` environment
/// variable, default 1 MiB).
pub struct Channel<T: Encode> {
    inner: Option<(foxglove::Channel<T>, StreamHandle)>,
}

impl<T: Encode> Channel<T> {
    /// Create a channel for streaming (with auto-flush).
    pub(crate) fn for_streaming(channel: foxglove::Channel<T>, handle: StreamHandle) -> Self {
        Self {
            inner: Some((channel, handle)),
        }
    }

    /// Create a channel for manifest mode (logging will panic).
    pub(crate) fn for_manifest() -> Self {
        Self { inner: None }
    }

    /// Log a message with the given timestamp.
    ///
    /// After logging, the buffer is automatically flushed if it exceeds the
    /// configured threshold.
    ///
    /// # Panics
    ///
    /// Panics if called on a channel created during manifest generation.
    pub fn log(&self, msg: &T, timestamp: impl ToUnixNanos) {
        let (channel, handle) = self
            .inner
            .as_ref()
            .expect("attempted to log on a channel not created for streaming");
        channel.log_with_time(msg, timestamp);
        handle.try_flush();
    }
}

// ---------------------------------------------------------------------------
// McapStream
// ---------------------------------------------------------------------------

/// A stream of MCAP bytes.
pub(crate) struct McapStream {
    receiver: Receiver<BytesMut>,
}

impl Stream for McapStream {
    type Item = Bytes;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match ready!(self.receiver.poll_recv(cx)) {
            Some(bytes) => Poll::Ready(Some(bytes.freeze())),
            None => Poll::Ready(None),
        }
    }
}

// ---------------------------------------------------------------------------
// create_stream
// ---------------------------------------------------------------------------

/// Default flush threshold (1 MiB).
const DEFAULT_FLUSH_THRESHOLD: usize = 1024 * 1024;

/// Create a new MCAP stream pair.
///
/// Reads the `FOXGLOVE_FLUSH_THRESHOLD` environment variable for the buffer flush
/// threshold in bytes. Defaults to 1 MiB (1048576) if not set.
pub(crate) fn create_stream() -> (StreamHandle, McapStream) {
    let flush_threshold = std::env::var("FOXGLOVE_FLUSH_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_FLUSH_THRESHOLD);

    let buffer = SharedBuffer::default();
    let context = Context::new();
    let writer = context
        .mcap_writer_with_options(McapWriteOptions::new().disable_seeking(true))
        .create(buffer.clone())
        .expect("valid MCAP writer configuration");

    let (sender, receiver) = tokio::sync::mpsc::channel(1);

    let handle = StreamHandle(Arc::new(StreamHandleInner {
        buffer,
        sender,
        writer: Mutex::new(Some(writer)),
        context,
        flush_threshold,
    }));

    (handle, McapStream { receiver })
}

impl crate::ChannelRegistry for StreamHandle {
    fn channel<T: Encode>(&mut self, topic: impl Into<String>) -> Channel<T> {
        let foxglove_channel = self.0.context.channel_builder(topic).build();
        Channel::for_streaming(foxglove_channel, self.clone())
    }
}
