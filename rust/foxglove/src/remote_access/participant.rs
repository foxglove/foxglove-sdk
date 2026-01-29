#[cfg(test)]
use std::sync::Arc;
use std::{
    borrow::Borrow,
    hash::{Hash, Hasher},
};

#[cfg(test)]
use bytes::Bytes;
use livekit::{id::ParticipantIdentity, ByteStreamWriter, StreamWriter};

use crate::remote_access::RemoteAccessError;

pub(crate) struct Participant {
    identity: ParticipantIdentity,
    writer: ParticipantWriter,
}

impl Participant {
    pub fn new(identity: ParticipantIdentity, writer: ParticipantWriter) -> Self {
        Self { identity, writer }
    }

    pub(crate) async fn send(&self, bytes: &[u8]) -> Result<(), RemoteAccessError> {
        self.writer.write(bytes).await
    }
}

impl PartialEq for Participant {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for Participant {}

impl Borrow<ParticipantIdentity> for Participant {
    fn borrow(&self) -> &ParticipantIdentity {
        &self.identity
    }
}

impl Hash for Participant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity.hash(state);
    }
}

impl std::fmt::Debug for Participant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Participant {{ identity: {:?} }}", self.identity)
    }
}

#[allow(dead_code)]
pub(crate) enum ParticipantWriter {
    Livekit(ByteStreamWriter),
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
