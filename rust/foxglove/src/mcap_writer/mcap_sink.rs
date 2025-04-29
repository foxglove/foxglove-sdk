//! [`Sink`] implementation for an MCAP writer.
use crate::{ChannelId, FoxgloveError, Metadata, RawChannel, Sink, SinkId};
use mcap::WriteOptions;
use parking_lot::Mutex;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{Seek, Write};
use std::sync::Arc;

type McapChannelId = u16;

struct WriterState<W: Write + Seek> {
    writer: mcap::Writer<W>,
    // ChannelId -> mcap file channel id.
    // Note that the underlying writer may re-use channel_ids based on the metadata of the channel,
    // so multiple `ChannelIds` may map to the same `McapChannelId`.
    channel_map: HashMap<ChannelId, McapChannelId>,
    // Current message sequence number for each channel.
    // Indexed by `McapChannelId` to ensure increasing sequence within each MCAP channel.
    channel_sequence: HashMap<McapChannelId, u32>,
}

impl<W: Write + Seek> WriterState<W> {
    fn new(writer: mcap::Writer<W>) -> Self {
        Self {
            writer,
            channel_map: HashMap::new(),
            channel_sequence: HashMap::new(),
        }
    }

    fn next_sequence(&mut self, channel_id: McapChannelId) -> u32 {
        *self
            .channel_sequence
            .entry(channel_id)
            .and_modify(|seq| *seq += 1)
            .or_insert(1)
    }

    fn log(
        &mut self,
        channel: &RawChannel,
        msg: &[u8],
        metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        let channel_id = channel.id();
        let mcap_channel_id = match self.channel_map.entry(channel_id) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let schema_id = if let Some(schema) = channel.schema() {
                    self.writer
                        .add_schema(&schema.name, &schema.encoding, &schema.data)
                        .map_err(FoxgloveError::from)?
                } else {
                    0 // 0 indicates a channel without a schema
                };

                let mcap_channel_id = self
                    .writer
                    .add_channel(
                        schema_id,
                        channel.topic(),
                        channel.message_encoding(),
                        channel.metadata(),
                    )
                    .map_err(FoxgloveError::from)?;

                entry.insert(mcap_channel_id);
                mcap_channel_id
            }
        };

        let sequence = self.next_sequence(mcap_channel_id);

        self.writer
            .write_to_known_channel(
                &mcap::records::MessageHeader {
                    channel_id: mcap_channel_id,
                    sequence,
                    log_time: metadata.log_time,
                    // Use log_time as publish_time (required when publish_time unavailable)
                    publish_time: metadata.log_time,
                },
                msg,
            )
            .map_err(FoxgloveError::from)
    }
}

pub struct McapSink<W: Write + Seek> {
    sink_id: SinkId,
    inner: Mutex<Option<WriterState<W>>>,
}
impl<W: Write + Seek> Debug for McapSink<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McapSink")
            .field("sink_id", &self.sink_id)
            .finish()
    }
}

impl<W: Write + Seek> McapSink<W> {
    /// Creates a new MCAP writer sink.
    pub fn new(writer: W, options: WriteOptions) -> Result<Arc<McapSink<W>>, FoxgloveError> {
        let mcap_writer = options.create(writer).map_err(FoxgloveError::from)?;
        let writer = Arc::new(Self {
            sink_id: SinkId::next(),
            inner: Mutex::new(Some(WriterState::new(mcap_writer))),
        });
        Ok(writer)
    }

    /// Finalizes the MCAP recording and flushes it to the file.
    ///
    /// Returns the inner writer that was passed to [`McapWriter::new`].
    pub fn finish(&self) -> Result<Option<W>, FoxgloveError> {
        let Some(mut writer) = self.inner.lock().take() else {
            return Ok(None);
        };
        writer.writer.finish()?;
        Ok(Some(writer.writer.into_inner()))
    }
}

impl<W: Write + Seek + Send> Sink for McapSink<W> {
    fn id(&self) -> SinkId {
        self.sink_id
    }

    fn log(
        &self,
        channel: &RawChannel,
        msg: &[u8],
        metadata: &Metadata,
    ) -> Result<(), FoxgloveError> {
        _ = metadata;
        let mut guard = self.inner.lock();
        let writer = guard.as_mut().ok_or(FoxgloveError::SinkClosed)?;
        writer.log(channel, msg, metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChannelBuilder, Context, Metadata, Schema};
    use mcap::McapError;
    use std::path::Path;
    use tempfile::NamedTempFile;

    fn new_test_channel(ctx: &Arc<Context>, topic: String, schema_name: String) -> Arc<RawChannel> {
        ChannelBuilder::new(topic)
            .context(ctx)
            .message_encoding("message_encoding")
            .schema(Schema::new(
                schema_name,
                "encoding",
                br#"{
                    "type": "object",
                    "properties": {
                        "msg": {"type": "string"},
                        "count": {"type": "number"},
                    },
                }"#,
            ))
            .metadata(maplit::btreemap! {"key".to_string() => "value".to_string()})
            .build_raw()
            .unwrap()
    }

    fn foreach_mcap_message<F>(path: &Path, mut f: F) -> Result<(), McapError>
    where
        F: FnMut(mcap::Message),
    {
        let contents = std::fs::read(path).map_err(McapError::Io)?;
        let stream = mcap::MessageStream::new(&contents)?;
        for msg_result in stream {
            f(msg_result?);
        }
        Ok(())
    }

    #[test]
    fn test_log_channels() {
        let ctx = Context::new();
        // Create two channels
        let ch1 = new_test_channel(&ctx, "foo".to_string(), "foo_schema".to_string());
        let ch2 = new_test_channel(&ctx, "bar".to_string(), "bar_schema".to_string());

        // Generate a temporary file path without creating the file
        let temp_file = NamedTempFile::new().expect("create tempfile");
        let temp_path = temp_file.path().to_owned();

        // Generate some unique metadata for each message
        let ch1_meta = &[Metadata { log_time: 3 }, Metadata { log_time: 6 }];
        let mut ch1_meta_iter = ch1_meta.iter();

        let ch2_meta = &[Metadata { log_time: 9 }, Metadata { log_time: 12 }];
        let mut ch2_meta_iter = ch2_meta.iter();

        // Log two messages to each channel, interleaved
        let writer =
            McapSink::new(&temp_file, WriteOptions::default()).expect("failed to create writer");
        writer
            .log(&ch1, b"msg1", &ch1_meta[0])
            .expect("failed to log to channel 1");
        writer
            .log(&ch2, b"msg2", &ch2_meta[0])
            .expect("failed to log to channel 2");
        writer
            .log(&ch1, b"msg3", &ch1_meta[1])
            .expect("failed to log to channel 1");
        writer
            .log(&ch2, b"msg4", &ch2_meta[1])
            .expect("failed to log to channel 2");
        writer.finish().expect("failed to finish recording");

        let ch1_msgs: &[&[u8]] = &[b"msg1", b"msg3"];
        let ch2_msgs: &[&[u8]] = &[b"msg2", b"msg4"];
        let mut ch1_msgs_iter = ch1_msgs.iter();
        let mut ch2_msgs_iter = ch2_msgs.iter();

        // Read the MCAP file and verify the contents
        foreach_mcap_message(&temp_path, |msg| {
            let channel_id = msg.channel.id;
            let payload = msg.data;
            match channel_id {
                1 => {
                    assert_eq!(
                        &payload,
                        ch1_msgs_iter.next().expect("unexpected message channel 1")
                    );
                    let metadata = ch1_meta_iter.next().expect("unexpected metadata channel 1");
                    assert_eq!(msg.publish_time, metadata.log_time); // publish_time == log_time
                    assert_eq!(msg.log_time, metadata.log_time);
                    assert_eq!(msg.channel.topic, "foo");
                    assert_eq!(
                        msg.channel.schema.as_ref().expect("missing schema").name,
                        "foo_schema"
                    );
                }
                2 => {
                    assert_eq!(
                        &payload,
                        ch2_msgs_iter.next().expect("unexpected message channel 2")
                    );
                    let metadata = ch2_meta_iter.next().expect("unexpected metadata channel 2");
                    assert_eq!(msg.publish_time, metadata.log_time); // publish_time == log_time
                    assert_eq!(msg.log_time, metadata.log_time);
                    assert_eq!(msg.channel.topic, "bar");
                    assert_eq!(
                        msg.channel.schema.as_ref().expect("missing schema").name,
                        "bar_schema"
                    );
                }
                _ => panic!("unexpected channel id: {}", channel_id),
            }
        })
        .expect("failed to read MCAP messages");
    }

    #[test]
    fn test_message_sequence_increases_by_channel() {
        let ctx = Context::new();

        // MCAP writer will re-use the same channel internally for ch2 & ch3 since topic and schema are the same.
        let ch1 = new_test_channel(&ctx, "foo".to_string(), "foo_schema".to_string());
        let ch2 = new_test_channel(&ctx, "bar".to_string(), "bar_schema".to_string());
        let ch3 = new_test_channel(&ctx, "bar".to_string(), "bar_schema".to_string());

        // Generate a temporary file path without creating the file
        let temp_file = NamedTempFile::new().expect("failed to create tempfile");
        let temp_path = temp_file.path().to_owned();

        let metadata = Metadata::default();
        let writer =
            McapSink::new(&temp_file, WriteOptions::default()).expect("failed to create writer");

        writer
            .log(&ch1, b"msg1", &metadata)
            .expect("failed to log to channel 1");
        writer
            .log(&ch2, b"msg2", &metadata)
            .expect("failed to log to channel 2");
        writer
            .log(&ch3, b"msg3", &metadata)
            .expect("failed to log to channel 3");
        writer
            .log(&ch1, b"msg4", &metadata)
            .expect("failed to log to channel 1");
        writer
            .log(&ch2, b"msg5", &metadata)
            .expect("failed to log to channel 2");
        writer
            .log(&ch2, b"msg6", &metadata)
            .expect("failed to log to channel 3");
        writer.finish().expect("failed to finish recording");

        let contents = std::fs::read(&temp_path)
            .map_err(McapError::Io)
            .expect("failed to read mcap");
        let stream = mcap::MessageStream::new(&contents).expect("failed to create message stream");
        let messages: Vec<mcap::Message> = stream
            .collect::<Result<Vec<_>, _>>()
            .expect("failed to collect messages");

        assert_eq!(messages.len(), 6);

        // Channel 2 and 3 share the same mcap_channel_id
        assert_eq!(messages[0].channel.id, 1);
        assert_eq!(messages[1].channel.id, 2);
        assert_eq!(messages[2].channel.id, 2);
        assert_eq!(messages[3].channel.id, 1);
        assert_eq!(messages[4].channel.id, 2);
        assert_eq!(messages[5].channel.id, 2);

        // Channel 1 has independent sequence numbers
        assert_eq!(messages[0].sequence, 1);
        assert_eq!(messages[3].sequence, 2);

        // Channel 2 and 3 share an MCAP channel_id, so increment together
        assert_eq!(messages[1].sequence, 1);
        assert_eq!(messages[2].sequence, 2);
        assert_eq!(messages[4].sequence, 3);
        assert_eq!(messages[5].sequence, 4);
    }
}
