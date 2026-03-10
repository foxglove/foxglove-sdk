//! MCAP Stream
//!
//! This module contains utilities for writing to stream of MCAP bytes, commonly used for streaming
//! responses from HTTP frameworks such as Axum.

use std::{
    fmt::Debug,
    io::{self, Seek, SeekFrom, Write},
    pin::Pin,
    sync::Arc,
    task::Poll,
};

use tokio::sync::mpsc::{Receiver as TokioReceiver, Sender as TokioSender};

use bytes::{Bytes, BytesMut};
use futures::{Stream, ready};
use parking_lot::Mutex;

use crate::{ChannelBuilder, Context, FoxgloveError, McapWriteOptions, McapWriterHandle};

#[derive(Default)]
struct Inner {
    buffer: BytesMut,
    position: u64,
}

#[derive(Default, Clone)]
struct SharedBuffer(Arc<Mutex<Inner>>);

impl Debug for SharedBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedBuffer").finish_non_exhaustive()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let inner = &mut *self.0.lock();
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
            _ => Err(std::io::Error::other("seek on unseekable file")),
        }
    }
}

/// Creates an [`McapStream`] and [`McapStreamHandle`] pair that can be used to encode logged
/// messages as a [`futures::Stream`] of MCAP bytes.
///
/// The returned [`McapStreamHandle`] can be used to create channels which will log messages to the
/// [`McapStream`]. [`McapStreamHandle::flush`] must be routinely called on the handle to push bytes
/// from the writer to the [`McapStream`]. When the recording is finished
/// [`McapStreamHandle::close`] must be called to ensure that all bytes have been flushed to the
/// [`McapStream`].
pub fn create_mcap_stream() -> (McapStreamHandle, McapStream) {
    let buffer = SharedBuffer::default();
    let context = Context::new();

    let writer = context
        .mcap_writer_with_options(McapWriteOptions::new().disable_seeking(true))
        .create(buffer.clone())
        .expect("writer has valid configuration");

    let (sender, receiver) = tokio::sync::mpsc::channel(1);

    let handle = McapStreamHandle {
        buffer,
        writer: Some(writer),
        sender,
        context,
    };

    (handle, McapStream { receiver })
}

/// A handle to an MCAP stream writer.
///
/// When this handle is dropped, the writer will unregister from the [`Context`] and stop logging
/// events. It will attempt to flush any buffered data but may fail if the [`McapStream`] is
/// currently full.
///
/// To ensure no data is lost, call the [`McapStreamHandle::close`] method instead of dropping.
#[must_use]
#[derive(Debug)]
pub struct McapStreamHandle {
    writer: Option<McapWriterHandle<SharedBuffer>>,
    buffer: SharedBuffer,
    sender: TokioSender<BytesMut>,
    context: Arc<Context>,
}

impl McapStreamHandle {
    /// Returns a channel builder for a channel in the stream writer.
    ///
    /// You should choose a unique topic name per channel for compatibility with the Foxglove app.
    pub fn channel_builder(&self, topic: impl Into<String>) -> ChannelBuilder {
        self.context.channel_builder(topic)
    }

    /// Stop logging events and flush any buffered data.
    ///
    /// This method will return an error if the MCAP writer fails to finish or if the
    /// [`McapStream`] has already been closed.
    pub async fn close(mut self) -> Result<(), FoxgloveError> {
        if let Some(writer) = self.writer.take() {
            if let Err(e) = writer.close() {
                // If an error occurred still flush the buffer. We'll likely get a truncated MCAP
                // but anything that was successfully written will be there.
                let _ = Self::flush_shared_buffer(&mut self.sender, &mut self.buffer).await;
                return Err(e);
            }
        }

        Ok(Self::flush_shared_buffer(&mut self.sender, &mut self.buffer).await?)
    }

    async fn flush_shared_buffer(
        sender: &mut TokioSender<BytesMut>,
        buffer: &mut SharedBuffer,
    ) -> io::Result<()> {
        let bytes = {
            let mut inner = buffer.0.lock();
            inner.buffer.split()
        };

        if bytes.is_empty() {
            return Ok(());
        }

        if sender.send(bytes).await.is_err() {
            return Err(std::io::Error::other("McapStream channel was closed"));
        }

        Ok(())
    }

    /// Get the current size of the buffer.
    ///
    /// This can be used in conjunction with [`McapStreamHandle::flush`] to ensure the buffer does
    /// not grow unbounded.
    pub fn buffer_size(&mut self) -> usize {
        self.buffer.0.lock().buffer.len()
    }

    /// Flush the buffer from the MCAP writer to the [`McapStream`].
    ///
    /// This method returns a future that will wait until the [`McapStream`] has capacity for the
    /// flushed buffer.
    pub async fn flush(&mut self) -> Result<(), FoxgloveError> {
        Self::flush_shared_buffer(&mut self.sender, &mut self.buffer).await?;
        Ok(())
    }
}

impl Drop for McapStreamHandle {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            if let Err(e) = writer.close() {
                tracing::warn!("{e}");
            }
        }

        let mut inner = self.buffer.0.lock();
        let buffer = inner.buffer.split();

        if !buffer.is_empty() {
            // When the handle is dropped try and send the final buffer. If the channel is full or
            // closed log as a warning.
            if let Err(e) = self.sender.try_send(buffer) {
                tracing::warn!("{e}");
            }
        }
    }
}

/// A stream of MCAP bytes from a writer.
pub struct McapStream {
    receiver: TokioReceiver<BytesMut>,
}

impl Stream for McapStream {
    type Item = Bytes;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let Some(bytes) = ready!(self.receiver.poll_recv(cx)) else {
            return Poll::Ready(None);
        };

        Poll::Ready(Some(bytes.freeze()))
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;
    use std::convert::Infallible;

    use crate::Encode;

    use super::*;

    struct Message {
        data: f64,
    }

    impl Encode for Message {
        type Error = Infallible;

        fn get_schema() -> Option<crate::Schema> {
            None
        }

        fn get_message_encoding() -> String {
            "foo".to_string()
        }

        fn encode(&self, buf: &mut impl bytes::BufMut) -> Result<(), Self::Error> {
            buf.put_f64(self.data);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_write_to_stream() {
        let (mut handle, mut stream) = create_mcap_stream();

        let channel = handle.channel_builder("/topic").build::<Message>();

        // Use another thread to write messages to the stream
        tokio::spawn(async move {
            for i in 0..100 {
                channel.log(&Message { data: i as f64 });
                handle.flush().await.unwrap();
            }

            handle.close().await.unwrap();
        });

        let mut mcap_bytes = vec![];

        // Consume the stream and write the output to a vector.
        //
        // This stream will commonly be returned from an Axum handler as a streaming response.
        while let Some(bytes) = stream.next().await {
            mcap_bytes.extend_from_slice(&bytes[..]);
        }

        // The stream produces a complete MCAP file.
        // Verify by loading the summary from the file.
        let summary = mcap::Summary::read(&mcap_bytes[..]).unwrap().unwrap();
        let stats = summary.stats.unwrap();

        assert_eq!(stats.message_count, 100);
        assert_eq!(stats.channel_count, 1);
    }
}
