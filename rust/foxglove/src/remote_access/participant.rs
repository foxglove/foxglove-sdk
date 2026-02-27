#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use bytes::Bytes;
use livekit::{ByteStreamWriter, StreamWriter, id::ParticipantIdentity};

use crate::remote_access::RemoteAccessError;

/// A participant in the remote access session.
///
/// A participant has an identity and a dedicated TCP-like binary stream for sending messages.
///
/// This is a place to store state specific to the participant.
pub(crate) struct Participant {
    identity: ParticipantIdentity,
    /// A reliable, ordered stream to send messages to just this participant
    writer: ParticipantWriter,
}

impl Participant {
    /// Creates a new participant.
    pub fn new(identity: ParticipantIdentity, writer: ParticipantWriter) -> Self {
        Self { identity, writer }
    }

    /// Returns the participant's identity.
    pub fn identity(&self) -> &ParticipantIdentity {
        &self.identity
    }

    /// Sends a message to the participant.
    ///
    /// The message is serialized and framed already and provided as a slice of bytes.
    pub(crate) async fn send(&self, bytes: &[u8]) -> Result<(), RemoteAccessError> {
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
    async fn write(&self, bytes: &[u8]) -> Result<(), RemoteAccessError> {
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
