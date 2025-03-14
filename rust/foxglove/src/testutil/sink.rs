use crate::channel::ChannelId;
use crate::sink::Sink;
use crate::{Channel, FoxgloveError, Metadata};
use parking_lot::Mutex;

pub struct MockSink;

impl Sink for MockSink {
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
    pub recorded: Mutex<Vec<LogCall>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self {
            recorded: Mutex::new(Vec::new()),
        }
    }
}

impl Sink for RecordingSink {
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

pub struct ErrorSink;

#[derive(Debug, thiserror::Error)]
struct StrError(&'static str);

impl std::fmt::Display for StrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl Sink for ErrorSink {
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
