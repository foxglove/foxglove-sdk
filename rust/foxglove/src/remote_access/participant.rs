use std::collections::HashSet;
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use bytes::Bytes;
use livekit::{id::ParticipantIdentity, ByteStreamWriter, StreamWriter};

use crate::remote_access::RemoteAccessError;
use crate::ChannelId;

/// A participant in the remote access session.
///
/// A participant has an identity and a dedicated TCP-like binary stream for sending messages.
///
/// This is a place to store state specific to the participant.
pub(crate) struct Participant {
    identity: ParticipantIdentity,
    /// A reliable, ordered stream to send messages to just this participant
    writer: ParticipantWriter,
    /// Channel IDs this participant is currently subscribed to.
    subscribed_channels: parking_lot::Mutex<HashSet<ChannelId>>,
}

impl Participant {
    /// Creates a new participant.
    pub fn new(identity: ParticipantIdentity, writer: ParticipantWriter) -> Self {
        Self {
            identity,
            writer,
            subscribed_channels: parking_lot::Mutex::new(HashSet::new()),
        }
    }

    /// Returns the participant's identity.
    #[expect(dead_code)]
    pub fn identity(&self) -> &ParticipantIdentity {
        &self.identity
    }

    /// Adds the given channel IDs to this participant's subscriptions.
    ///
    /// Returns the channel IDs that were newly subscribed (excludes already-subscribed channels).
    pub fn subscribe(&self, channel_ids: &[ChannelId]) -> Vec<ChannelId> {
        let mut subscribed = self.subscribed_channels.lock();
        channel_ids
            .iter()
            .copied()
            .filter(|id| subscribed.insert(*id))
            .collect()
    }

    /// Removes the given channel IDs from this participant's subscriptions.
    ///
    /// Returns the channel IDs that were actually unsubscribed (excludes channels not subscribed).
    pub fn unsubscribe(&self, channel_ids: &[ChannelId]) -> Vec<ChannelId> {
        let mut subscribed = self.subscribed_channels.lock();
        channel_ids
            .iter()
            .copied()
            .filter(|id| subscribed.remove(id))
            .collect()
    }

    /// Returns true if this participant is subscribed to the given channel.
    pub fn is_subscribed(&self, channel_id: ChannelId) -> bool {
        self.subscribed_channels.lock().contains(&channel_id)
    }

    /// Removes all subscriptions for this participant and returns the previously subscribed channel IDs.
    #[expect(dead_code)]
    pub fn unsubscribe_all(&self) -> Vec<ChannelId> {
        let mut subscribed = self.subscribed_channels.lock();
        subscribed.drain().collect()
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
        write!(f, "Participant {{ identity: {:?} }}", self.identity)
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
