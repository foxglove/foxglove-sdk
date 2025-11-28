use std::{
    fmt::Debug,
    io::{self, Seek, SeekFrom, Write},
    pin::Pin,
    sync::Arc,
    task::Poll,
};

use tokio::sync::mpsc::{error::SendError, Receiver as TokioReceiver, Sender as TokioSender};

use bytes::{Bytes, BytesMut};
use futures::{ready, Stream};
use parking_lot::Mutex;

use crate::{Context, FoxgloveError, McapWriteOptions, McapWriter, McapWriterHandle};

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
            _ => Err(std::io::Error::other("seek on unseekable file")),
        }
    }
}

/// An MCAP writer for logging events and encoding them as a stream of bytes.
///
/// ### Buffering
///
/// Logged messages are buffered in memory until a call to [`McapStreamHandle::flush`] is called.
/// If this method is not called messages will buffer indefinitely.
#[must_use]
#[derive(Debug)]
pub struct McapStreamBuilder {
    writer: McapWriter,
}

impl McapStreamBuilder {
    pub(crate) fn new(context: &Arc<Context>) -> Self {
        Self {
            writer: context.mcap_writer_with_options(McapWriteOptions::new().disable_seeking(true)),
        }
    }

    /// Begin logging events to the returned [`McapStream`].
    ///
    /// This method returns both an [`McapStreamHandle`] and an [`McapStream`]. The handle must
    /// routinely call [`McapStreamHandle::flush`] to push bytes from the writer to the
    /// [`McapStream`]. When the recording is finished [`McapStreamHandle::close`] must be called
    /// to ensure that all bytes have been flushed to the [`McapStream`].
    pub fn create(self) -> Result<(McapStreamHandle, McapStream), FoxgloveError> {
        let buffer = SharedBuffer::default();
        let writer = self.writer.create(buffer.clone())?;

        let (sender, receiver) = tokio::sync::mpsc::channel(1);

        Ok((
            McapStreamHandle {
                buffer,
                writer: Some(writer),
                sender,
            },
            McapStream { receiver },
        ))
    }
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
}

impl McapStreamHandle {
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

        if let Err(SendError(bytes)) = sender.send(bytes).await {
            let mut inner = buffer.0.lock();
            inner.buffer.unsplit(bytes);
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
            if let Err(e) = writer.finish() {
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
    use std::convert::Infallible;

    use futures::StreamExt;

    use crate::{Context, Encode};

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
        let context = Context::new();

        let channel = context.channel_builder("/topic").build::<Message>();

        let (mut handle, mut stream) = context.mcap_stream().create().unwrap();

        tokio::spawn(async move {
            for i in 0..100 {
                channel.log(&Message { data: i as f64 });
                handle.flush().await.unwrap();
            }

            handle.close().await.unwrap();
        });

        let mut out = vec![];

        while let Some(bytes) = stream.next().await {
            out.extend_from_slice(&bytes[..]);
        }

        let summary = mcap::Summary::read(&out[..]).unwrap().unwrap();
        let stats = summary.stats.unwrap();

        assert_eq!(stats.message_count, 100);
        assert_eq!(stats.channel_count, 1);
    }

    #[tokio::test]
    async fn test_write_even_when_channel_is_closed() {
        let context = Context::new();

        let channel = context.channel_builder("/topic").build::<Message>();

        let (mut handle, stream) = context.mcap_stream().create().unwrap();

        drop(stream);

        for i in 0..100 {
            channel.log(&Message { data: i as f64 });
            // every attempt to flush should fail, but the buffer is preserved
            handle.flush().await.unwrap_err();
        }

        let buffer = handle.writer.take().unwrap().finish().unwrap().unwrap();

        let summary = mcap::Summary::read(&buffer.0.lock().buffer[..])
            .unwrap()
            .unwrap();

        let stats = summary.stats.unwrap();

        assert_eq!(stats.message_count, 100);
        assert_eq!(stats.channel_count, 1);
    }
}
