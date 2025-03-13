use crate::channel::ChannelId;
use crate::{Channel, FoxgloveError, Metadata, Sink, SinkId};
use parking_lot::Mutex;

pub struct MockSink(SinkId);
impl Default for MockSink {
    fn default() -> Self {
        Self(SinkId::next())
    }
}

impl Sink for MockSink {
    fn id(&self) -> SinkId {
        self.0
    }

    fn log(
        &self,
        _channel: &Channel,
        _msg: &[u8],
        _metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        Ok(())
    }
}

pub struct LogCall {
    pub channel_id: ChannelId,
    pub msg: Vec<u8>,
    pub metadata: Metadata,
}

pub struct RecordingSink {
    id: SinkId,
    auto_subscribe: bool,
    pub recorded: Mutex<Vec<LogCall>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self {
            id: SinkId::next(),
            auto_subscribe: true,
            recorded: Mutex::new(Vec::new()),
        }
    }

    pub fn auto_subscribe(mut self, value: bool) -> Self {
        self.auto_subscribe = value;
        self
    }
}

impl Sink for RecordingSink {
    fn id(&self) -> SinkId {
        self.id
    }

    fn auto_subscribe(&self) -> bool {
        self.auto_subscribe
    }

    fn log(&self, channel: &Channel, msg: &[u8], metadata: &Metadata) -> Result<(), FoxgloveError> {
        let mut recorded = self.recorded.lock();
        recorded.push(LogCall {
            channel_id: channel.id(),
            msg: msg.to_vec(),
            metadata: *metadata,
        });
        Ok(())
    }
}

pub struct ErrorSink(SinkId);
impl Default for ErrorSink {
    fn default() -> Self {
        Self(SinkId::next())
    }
}

#[derive(Debug, thiserror::Error)]
struct StrError(&'static str);

impl std::fmt::Display for StrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl Sink for ErrorSink {
    fn id(&self) -> SinkId {
        self.0
    }

    fn log(
        &self,
        _channel: &Channel,
        _msg: &[u8],
        _metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        Err(FoxgloveError::Unspecified(Box::new(StrError(
            "ErrorSink always fails",
        ))))
    }
}
