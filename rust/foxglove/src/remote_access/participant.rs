#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use bytes::Bytes;
use livekit::{ByteStreamWriter, StreamWriter, id::ParticipantIdentity};

use crate::remote_access::RemoteAccessError;
use crate::remote_common::ClientId;

type Result<T> = std::result::Result<T, Box<RemoteAccessError>>;

/// A participant in the remote access session.
///
/// A participant has an identity and a dedicated TCP-like binary stream for sending messages.
///
/// This is a place to store state specific to the participant.
pub(crate) struct Participant {
    id: ClientId,
    identity: ParticipantIdentity,
    /// A reliable, ordered stream to send messages to just this participant
    writer: ParticipantWriter,
}

/// A per-channel writer for data plane messages.
///
/// Wraps a `ByteStreamWriter` addressed to a specific set of participants, together with
/// the subscription version at which the writer was created. The version is used for a
/// cheap staleness check: if the current subscription version for the channel differs from
/// `version`, the writer must be replaced.
pub(crate) struct ChannelWriter {
    inner: ChannelWriterInner,
    /// Subscription version this writer was opened for.
    version: u32,
}

impl ChannelWriter {
    /// Creates a new `ChannelWriter` wrapping a LiveKit byte stream writer.
    pub fn new(writer: ByteStreamWriter, version: u32) -> Self {
        Self {
            inner: ChannelWriterInner::Livekit(writer),
            version,
        }
    }

    /// Creates a `ChannelWriter` backed by a test writer.
    #[cfg(test)]
    pub fn test(writer: Arc<TestChannelWriter>, version: u32) -> Self {
        Self {
            inner: ChannelWriterInner::Test(writer),
            version,
        }
    }

    /// Returns the subscription version this writer was created for.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Writes bytes to the channel's byte stream.
    pub async fn write(&self, bytes: &[u8]) -> Result<()> {
        self.inner.write(bytes).await
    }
}

enum ChannelWriterInner {
    Livekit(ByteStreamWriter),
    #[allow(dead_code)]
    #[cfg(test)]
    Test(Arc<TestChannelWriter>),
}

impl ChannelWriterInner {
    async fn write(&self, bytes: &[u8]) -> Result<()> {
        match self {
            ChannelWriterInner::Livekit(stream) => stream.write(bytes).await.map_err(|e| e.into()),
            #[cfg(test)]
            ChannelWriterInner::Test(writer) => writer.write(bytes),
        }
    }
}

impl Participant {
    /// Creates a new participant.
    pub fn new(identity: ParticipantIdentity, writer: ParticipantWriter) -> Self {
        Self {
            id: ClientId::next(),
            identity,
            writer,
        }
    }

    /// Returns the locally-significant client ID.
    pub fn id(&self) -> ClientId {
        self.id
    }

    /// Returns the participant's identity.
    pub fn identity(&self) -> &ParticipantIdentity {
        &self.identity
    }

    /// Sends a message to the participant.
    ///
    /// The message is serialized and framed already and provided as a slice of bytes.
    pub(crate) async fn send(&self, bytes: &[u8]) -> Result<()> {
        self.writer.write(bytes).await
    }
}

impl std::fmt::Debug for Participant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Participant")
            .field("identity", &self.identity)
            .finish()
    }
}

impl std::fmt::Display for Participant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Participant({})", self.identity)
    }
}

/// A writer for a participant.
///
/// Wraps an ordered, reliable byte stream to one specific participant.
///
/// Mocked with a TestByteStreamWriter for tests.
pub(crate) enum ParticipantWriter {
    Livekit(ByteStreamWriter),
    #[allow(dead_code)]
    #[cfg(test)]
    Test(Arc<TestByteStreamWriter>),
}

impl ParticipantWriter {
    async fn write(&self, bytes: &[u8]) -> Result<()> {
        match self {
            ParticipantWriter::Livekit(stream) => stream.write(bytes).await.map_err(|e| e.into()),
            #[cfg(test)]
            ParticipantWriter::Test(writer) => {
                writer.record(bytes);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestByteStreamWriter {
    writes: parking_lot::Mutex<Vec<Bytes>>,
}

#[cfg(test)]
impl TestByteStreamWriter {
    fn record(&self, data: &[u8]) {
        self.writes.lock().push(Bytes::copy_from_slice(data));
    }

    #[allow(dead_code)]
    pub(crate) fn writes(&self) -> Vec<Bytes> {
        std::mem::take(&mut self.writes.lock())
    }
}

/// A test double for channel-level byte stream writes.
///
/// Records all writes and can be configured to fail (via [`TestChannelWriter::new_failing`]).
#[cfg(test)]
pub(crate) struct TestChannelWriter {
    writes: parking_lot::Mutex<Vec<Bytes>>,
    fail: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl Default for TestChannelWriter {
    fn default() -> Self {
        Self {
            writes: parking_lot::Mutex::new(Vec::new()),
            fail: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[cfg(test)]
impl TestChannelWriter {
    /// Creates a writer that always returns an error.
    pub fn new_failing() -> Self {
        Self {
            writes: parking_lot::Mutex::new(Vec::new()),
            fail: std::sync::atomic::AtomicBool::new(true),
        }
    }

    fn write(&self, data: &[u8]) -> Result<()> {
        if self.fail.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Box::new(RemoteAccessError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "test write failure",
            ))));
        }
        self.writes.lock().push(Bytes::copy_from_slice(data));
        Ok(())
    }

    pub fn writes(&self) -> Vec<Bytes> {
        self.writes.lock().clone()
    }
}
