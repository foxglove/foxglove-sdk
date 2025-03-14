//! [`Sink`] implementation for an MCAP writer.
use crate::channel::Channel;
use crate::channel::ChannelId;
use crate::metadata::Metadata;
use crate::sink::Sink;
use crate::FoxgloveError;
use mcap::WriteOptions;
use parking_lot::Mutex;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::{Seek, Write};
use std::sync::Arc;

struct WriterState<W: Write + Seek> {
    writer: mcap::Writer<W>,
    // ChannelId -> mcap file channel id
    channel_map: HashMap<ChannelId, u16>,
}

impl<W: Write + Seek> WriterState<W> {
    fn new(writer: mcap::Writer<W>) -> Self {
        Self {
            writer,
            channel_map: HashMap::new(),
        }
    }

    fn log(
        &mut self,
        channel: &Channel,
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
                        &channel.topic,
                        &channel.message_encoding,
                        &channel.metadata,
                    )
                    .map_err(FoxgloveError::from)?;

                entry.insert(mcap_channel_id);
                mcap_channel_id
            }
        };

        self.writer
            .write_to_known_channel(
                &mcap::records::MessageHeader {
                    channel_id: mcap_channel_id,
                    sequence: metadata.sequence,
                    log_time: metadata.log_time,
                    publish_time: metadata.publish_time,
                },
                msg,
            )
            .map_err(FoxgloveError::from)
    }
}

pub struct McapSink<W: Write + Seek>(Mutex<Option<WriterState<W>>>);

impl<W: Write + Seek> McapSink<W> {
    /// Creates a new MCAP writer sink.
    pub fn new(writer: W, options: WriteOptions) -> Result<Arc<McapSink<W>>, FoxgloveError> {
        let mcap_writer = options.create(writer).map_err(FoxgloveError::from)?;
        let writer = Arc::new(Self(Mutex::new(Some(WriterState::new(mcap_writer)))));
        Ok(writer)
    }

    /// Finalizes the MCAP recording and flushes it to the file.
    ///
    /// Returns the inner writer that was passed to [`McapWriter::new`].
    pub fn finish(&self) -> Result<Option<W>, FoxgloveError> {
        let Some(mut writer) = self.0.lock().take() else {
            return Ok(None);
        };
        writer.writer.finish()?;
        Ok(Some(writer.writer.into_inner()))
    }
}

impl<W: Write + Seek + Send> Sink for McapSink<W> {
    fn log(&self, channel: &Channel, msg: &[u8], metadata: &Metadata) -> Result<(), FoxgloveError> {
        _ = metadata;
        let mut guard = self.0.lock();
        let writer = guard.as_mut().ok_or(FoxgloveError::SinkClosed)?;
        writer.log(channel, msg, metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_sink_set::LogSinkSet;
    use crate::{collection, Metadata, Schema};
    use mcap::McapError;
    use std::path::Path;
    use std::sync::atomic::AtomicU32;
    use tempfile::NamedTempFile;

    fn new_test_channel(id: u64, topic: String, name: String) -> Arc<Channel> {
        Arc::new(Channel {
            sinks: LogSinkSet::new(),
            id: ChannelId::new(id),
            message_sequence: AtomicU32::new(1),
            topic,
            message_encoding: "message_encoding".to_string(),
            schema: Some(Schema::new(
                name,
                "encoding",
                br#"{
                    "type": "object",
                    "properties": {
                        "msg": {"type": "string"},
                        "count": {"type": "number"},
                    },
                }"#,
            )),
            metadata: collection! {"key".to_string() => "value".to_string()},
        })
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
        // Create two channels
        let ch1 = new_test_channel(1, "foo".to_string(), "foo_schema".to_string());
        let ch2 = new_test_channel(2, "bar".to_string(), "bar_schema".to_string());

        // Generate a temporary file path without creating the file
        let temp_file = NamedTempFile::new().expect("create tempfile");
        let temp_path = temp_file.path().to_owned();

        // Generate some unique metadata for each message
        let ch1_meta = &[
            Metadata {
                sequence: 1,
                publish_time: 2,
                log_time: 3,
            },
            Metadata {
                sequence: 4,
                publish_time: 5,
                log_time: 6,
            },
        ];
        let mut ch1_meta_iter = ch1_meta.iter();

        let ch2_meta = &[
            Metadata {
                sequence: 7,
                publish_time: 8,
                log_time: 9,
            },
            Metadata {
                sequence: 10,
                publish_time: 11,
                log_time: 12,
            },
        ];
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
            dbg!(&msg);
            let channel_id = msg.channel.id;
            let payload = msg.data;
            match channel_id {
                1 => {
                    assert_eq!(
                        &payload,
                        ch1_msgs_iter.next().expect("unexpected message channel 1")
                    );
                    let metadata = ch1_meta_iter.next().expect("unexpected metadata channel 1");
                    assert_eq!(msg.sequence, metadata.sequence);
                    assert_eq!(msg.publish_time, metadata.publish_time);
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
                    assert_eq!(msg.sequence, metadata.sequence);
                    assert_eq!(msg.publish_time, metadata.publish_time);
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
}
