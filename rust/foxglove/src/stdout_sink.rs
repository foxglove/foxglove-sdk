use std::sync::{Arc, Weak};

use crate::{Context, FoxgloveError, Metadata, RawChannel, Sink, SinkId};

#[doc(hidden)]
pub struct StdoutSink {
    id: SinkId,
    context: Weak<Context>,
}

impl StdoutSink {
    pub fn new() -> Arc<Self> {
        let context = Context::get_default();
        let sink = Arc::new(Self {
            id: SinkId::next(),
            context: Arc::downgrade(&context),
        });
        context.add_sink(sink.clone());
        sink
    }
}

impl Sink for StdoutSink {
    fn id(&self) -> SinkId {
        self.id
    }

    fn log(
        &self,
        _channel: &RawChannel,
        msg: &[u8],
        _metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        unsafe { foxglove_agent::foxglove_log_to_stdout(msg.as_ptr(), msg.len()) };
        Ok(())
    }
}

impl Drop for StdoutSink {
    fn drop(&mut self) {
        let context = self.context.upgrade();
        if let Some(context) = context {
            context.remove_sink(self.id);
        }
    }
}
