//! Async MCAP sink that doesn't block on disk I/O.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::io::{Seek, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::{get_runtime_handle, ChannelDescriptor, ChannelId, FoxgloveError, Metadata, RawChannel, Sink, SinkChannelFilter, SinkId};

type McapChannelId = u16;

/// Maximum number of messages that can be queued before new messages are dropped.
const QUEUE_CAPACITY: usize = 1024;

/// A queued log message.
struct QueuedLog {
    descriptor: ChannelDescriptor,
    msg: Box<[u8]>,
    metadata: Metadata,
}

/// Commands for the background writer task.
enum WriteCommand<W> {
    Log(QueuedLog),
    Metadata { name: String, data: BTreeMap<String, String> },
    Finish(tokio::sync::oneshot::Sender<Result<W, FoxgloveError>>),
}

/// Background loop that processes queued write commands.
/// Reads from the channel and writes to the file.
async fn run_writer_loop<W: Write + Seek>(
    rx: flume::Receiver<WriteCommand<W>>,
    mut writer: mcap::Writer<W>,
) {
    let mut channel_map: HashMap<ChannelId, Option<McapChannelId>> = HashMap::new();
    let mut channel_seq: HashMap<McapChannelId, u32> = HashMap::new();

    while let Ok(cmd) = rx.recv_async().await {
        match cmd {
            WriteCommand::Log(log) => {
                if let Err(e) = write_log(&mut writer, &mut channel_map, &mut channel_seq, &log) {
                    tracing::error!("MCAP write error: {e}");
                }
            }
            WriteCommand::Metadata { name, data } => {
                let _ = writer.write_metadata(&mcap::records::Metadata { name, metadata: data });
            }
            WriteCommand::Finish(done) => {
                let result = writer.finish().map_err(FoxgloveError::from);
                let _ = done.send(result.map(|_summary| writer.into_inner()));
                return;
            }
        }
    }
    let _ = writer.finish();
}

fn write_log<W: Write + Seek>(
    writer: &mut mcap::Writer<W>,
    channel_map: &mut HashMap<ChannelId, Option<McapChannelId>>,
    channel_seq: &mut HashMap<McapChannelId, u32>,
    log: &QueuedLog,
) -> Result<(), FoxgloveError> {
    let mcap_id = match channel_map.entry(log.descriptor.id()) {
        Entry::Occupied(e) => *e.get(),
        Entry::Vacant(e) => {
            let schema_id = if let Some(s) = log.descriptor.schema() {
                writer.add_schema(&s.name, &s.encoding, &s.data)?
            } else {
                0
            };
            let id = writer.add_channel(
                schema_id,
                log.descriptor.topic(),
                log.descriptor.message_encoding(),
                log.descriptor.metadata(),
            )?;
            e.insert(Some(id));
            Some(id)
        }
    };

    if let Some(id) = mcap_id {
        let seq = channel_seq.entry(id).and_modify(|s| *s += 1).or_insert(1);
        writer.write_to_known_channel(
            &mcap::records::MessageHeader {
                channel_id: id,
                sequence: *seq,
                log_time: log.metadata.log_time,
                publish_time: log.metadata.log_time,
            },
            &log.msg,
        )?;
    }
    Ok(())
}

/// An MCAP sink that writes asynchronously in a background task.
///
/// Unlike the synchronous `McapSink`, this version queues messages and writes
/// them asynchronously, so `log()` never blocks on disk I/O.
pub(super) struct AsyncMcapSink<W: Write + Seek + Send + 'static> {
    sink_id: SinkId,
    tx: flume::Sender<WriteCommand<W>>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    /// Tracks whether finish() was already called (to avoid double-finish in Drop)
    finished: AtomicBool,
}

impl<W: Write + Seek + Send + 'static> AsyncMcapSink<W> {
    /// Creates a new async sink. Spawns a background tokio task for writes.
    pub fn new(
        writer: W,
        options: mcap::WriteOptions,
        channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    ) -> Result<Arc<Self>, FoxgloveError> {
        let mcap_writer = options.create(writer)?;
        let (tx, rx) = flume::bounded::<WriteCommand<W>>(QUEUE_CAPACITY);

        get_runtime_handle().spawn(async move {
            run_writer_loop(rx, mcap_writer).await;
        });

        Ok(Arc::new(Self {
            sink_id: SinkId::next(),
            tx,
            channel_filter,
            finished: AtomicBool::new(false),
        }))
    }

    /// Waits for all queued writes to complete, closes the file, and returns the writer.
    pub async fn finish(&self) -> Result<W, FoxgloveError> {
        if self.finished.swap(true, Ordering::SeqCst) {
            return Err(FoxgloveError::SinkClosed);
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(WriteCommand::Finish(tx)).is_err() {
            return Err(FoxgloveError::SinkClosed);
        }
        rx.await.unwrap_or(Err(FoxgloveError::SinkClosed))
    }

    /// Blocks until all queued writes complete, closes the file, and returns the writer.
    /// Called automatically when the writer is dropped.
    pub fn finish_blocking(&self) -> Result<W, FoxgloveError> {
        if self.finished.swap(true, Ordering::SeqCst) {
            return Err(FoxgloveError::SinkClosed);
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        if self.tx.send(WriteCommand::Finish(tx)).is_err() {
            return Err(FoxgloveError::SinkClosed);
        }
        rx.blocking_recv().unwrap_or(Err(FoxgloveError::SinkClosed))
    }

    /// Writes metadata (queued, non-blocking).
    pub fn write_metadata(&self, name: &str, data: BTreeMap<String, String>) -> Result<(), FoxgloveError> {
        if self.finished.load(Ordering::SeqCst) {
            return Err(FoxgloveError::SinkClosed);
        }
        if !data.is_empty() {
            let _ = self.tx.try_send(WriteCommand::Metadata { name: name.into(), data });
        }
        Ok(())
    }
}

impl<W: Write + Seek + Send + 'static> Sink for AsyncMcapSink<W> {
    fn id(&self) -> SinkId {
        self.sink_id
    }

    fn log(&self, channel: &RawChannel, msg: &[u8], metadata: &Metadata) -> Result<(), FoxgloveError> {
        if self.finished.load(Ordering::SeqCst) {
            return Err(FoxgloveError::SinkClosed);
        }
        if let Some(f) = &self.channel_filter {
            if !f.should_subscribe(channel.descriptor()) {
                return Ok(());
            }
        }
        let _ = self.tx.try_send(WriteCommand::Log(QueuedLog {
            descriptor: channel.descriptor().clone(),
            msg: msg.into(),
            metadata: *metadata,
        }));
        Ok(())
    }
}

impl<W: Write + Seek + Send + 'static> Drop for AsyncMcapSink<W> {
    fn drop(&mut self) {
        if !self.finished.load(Ordering::SeqCst) {
            if let Err(e) = self.finish_blocking() {
                tracing::warn!("Error finishing MCAP file on drop: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChannelBuilder, Context, Schema};
    use mcap::WriteOptions;
    use std::path::Path;
    use tempfile::NamedTempFile;

    fn new_test_channel(ctx: &Arc<Context>, topic: &str, schema_name: &str) -> Arc<RawChannel> {
        ChannelBuilder::new(topic)
            .context(ctx)
            .message_encoding("message_encoding")
            .schema(Schema::new(
                schema_name,
                "encoding",
                br#"{"type": "object"}"#,
            ))
            .metadata(maplit::btreemap! {"key".to_string() => "value".to_string()})
            .build_raw()
            .unwrap()
    }

    fn read_mcap_messages(path: &Path) -> Vec<mcap::Message<'static>> {
        let contents = std::fs::read(path).expect("failed to read file");
        let contents = Box::leak(contents.into_boxed_slice());
        mcap::MessageStream::new(contents)
            .expect("failed to create stream")
            .collect::<Result<Vec<_>, _>>()
            .expect("failed to collect messages")
    }

    /// Tests that messages are correctly logged to multiple channels.
    #[tokio::test]
    async fn test_async_log_channels() {
        let ctx = Context::new();
        let ch1 = new_test_channel(&ctx, "foo", "foo_schema");
        let ch2 = new_test_channel(&ctx, "bar", "bar_schema");

        let temp_file = NamedTempFile::new().expect("create tempfile");
        let temp_path = temp_file.path().to_owned();
        let file = temp_file.reopen().expect("reopen");

        let sink = AsyncMcapSink::new(file, WriteOptions::default(), None)
            .expect("failed to create sink");

        sink.log(&ch1, b"msg1", &Metadata { log_time: 100 }).unwrap();
        sink.log(&ch2, b"msg2", &Metadata { log_time: 200 }).unwrap();
        sink.log(&ch1, b"msg3", &Metadata { log_time: 300 }).unwrap();
        sink.log(&ch2, b"msg4", &Metadata { log_time: 400 }).unwrap();

        sink.finish().await.expect("failed to finish");

        let messages = read_mcap_messages(&temp_path);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].channel.topic, "foo");
        assert_eq!(messages[0].data.as_ref(), b"msg1");
        assert_eq!(messages[1].channel.topic, "bar");
        assert_eq!(messages[2].channel.topic, "foo");
        assert_eq!(messages[3].channel.topic, "bar");
    }

    /// Tests that finish_blocking works from sync context.
    #[test]
    fn test_async_finish_blocking() {
        let temp_file = NamedTempFile::new().expect("create tempfile");
        let temp_path = temp_file.path().to_owned();
        let file = temp_file.reopen().expect("reopen");

        let ctx = Context::new();
        let ch = new_test_channel(&ctx, "test", "schema");

        let sink = AsyncMcapSink::new(file, WriteOptions::default(), None)
            .expect("failed to create sink");

        sink.log(&ch, b"blocking_test", &Metadata { log_time: 42 }).unwrap();
        sink.finish_blocking().expect("finish_blocking should succeed");

        let messages = read_mcap_messages(&temp_path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].data.as_ref(), b"blocking_test");
    }

    /// Tests that dropping the sink without calling finish still produces a valid file.
    #[test]
    fn test_async_drop_finishes_file() {
        let temp_file = NamedTempFile::new().expect("create tempfile");
        let temp_path = temp_file.path().to_owned();
        let file = temp_file.reopen().expect("reopen");

        let ctx = Context::new();
        let ch = new_test_channel(&ctx, "test", "schema");

        {
            let sink = AsyncMcapSink::new(file, WriteOptions::default(), None)
                .expect("failed to create sink");

            sink.log(&ch, b"drop_test", &Metadata { log_time: 99 }).unwrap();
            // Don't call finish - let Drop handle it
        }

        let messages = read_mcap_messages(&temp_path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].data.as_ref(), b"drop_test");
    }

    /// Stress test comparing sync vs async write performance.
    ///
    /// Run with: cargo test -p foxglove stress_test --release -- --ignored --nocapture
    #[test]
    #[ignore] // Run manually with --ignored flag
    fn stress_test_sync_vs_async() {
        use crate::mcap_writer::mcap_sink::McapSink;
        use std::time::Instant;

        let message = vec![0u8; 1024]; // 1KB message
        let ctx = Context::new();
        let ch = new_test_channel(&ctx, "stress", "schema");

        println!("\n=== Stress Test: Sync vs Async MCAP Writing ===\n");

        // =====================================================================
        // TEST 1: Small batch (fits in queue) - fair comparison
        // =====================================================================
        const SMALL_BATCH: usize = QUEUE_CAPACITY / 2; // Less than queue size

        let temp_sync = NamedTempFile::new().expect("create tempfile");
        let sync_sink = McapSink::new(
            temp_sync.reopen().expect("reopen"),
            WriteOptions::default(),
            None,
        ).expect("create sync sink");

        let sync_start = Instant::now();
        for i in 0..SMALL_BATCH {
            sync_sink.log(&ch, &message, &Metadata { log_time: i as u64 }).unwrap();
        }
        let sync_log_time = sync_start.elapsed();
        sync_sink.finish().expect("finish sync");
        let sync_total_time = sync_start.elapsed();
        let sync_count = read_mcap_messages(temp_sync.path()).len();

        let temp_async = NamedTempFile::new().expect("create tempfile");
        let async_sink = AsyncMcapSink::new(
            temp_async.reopen().expect("reopen"),
            WriteOptions::default(),
            None,
        ).expect("create async sink");

        let async_start = Instant::now();
        for i in 0..SMALL_BATCH {
            async_sink.log(&ch, &message, &Metadata { log_time: i as u64 }).unwrap();
        }
        let async_log_time = async_start.elapsed();
        async_sink.finish_blocking().expect("finish async");
        let async_total_time = async_start.elapsed();
        let async_count = read_mcap_messages(temp_async.path()).len();

        println!("TEST 1: Small batch ({} messages x 1KB, fits in queue)", SMALL_BATCH);
        println!("  SYNC:  log={:?}, total={:?}, wrote {} msgs", sync_log_time, sync_total_time, sync_count);
        println!("  ASYNC: log={:?}, total={:?}, wrote {} msgs", async_log_time, async_total_time, async_count);
        println!("  Speedup: {:.1}x faster log time", sync_log_time.as_secs_f64() / async_log_time.as_secs_f64());
        println!();

        // =====================================================================
        // TEST 2: Large batch (exceeds queue) - shows drop behavior
        // =====================================================================
        const LARGE_BATCH: usize = 100_000; // Much larger than QUEUE_CAPACITY

        let temp_sync2 = NamedTempFile::new().expect("create tempfile");
        let sync_sink2 = McapSink::new(
            temp_sync2.reopen().expect("reopen"),
            WriteOptions::default(),
            None,
        ).expect("create sync sink");

        let sync_start2 = Instant::now();
        for i in 0..LARGE_BATCH {
            sync_sink2.log(&ch, &message, &Metadata { log_time: i as u64 }).unwrap();
        }
        let sync_log_time2 = sync_start2.elapsed();
        sync_sink2.finish().expect("finish sync");
        let sync_total_time2 = sync_start2.elapsed();
        let sync_count2 = read_mcap_messages(temp_sync2.path()).len();

        let temp_async2 = NamedTempFile::new().expect("create tempfile");
        let async_sink2 = AsyncMcapSink::new(
            temp_async2.reopen().expect("reopen"),
            WriteOptions::default(),
            None,
        ).expect("create async sink");

        let async_start2 = Instant::now();
        for i in 0..LARGE_BATCH {
            async_sink2.log(&ch, &message, &Metadata { log_time: i as u64 }).unwrap();
        }
        let async_log_time2 = async_start2.elapsed();
        async_sink2.finish_blocking().expect("finish async");
        let async_total_time2 = async_start2.elapsed();
        let async_count2 = read_mcap_messages(temp_async2.path()).len();

        println!("TEST 2: Large batch ({} messages x 1KB, exceeds queue of {})", LARGE_BATCH, QUEUE_CAPACITY);
        println!("  SYNC:  log={:?}, total={:?}, wrote {} msgs", sync_log_time2, sync_total_time2, sync_count2);
        println!("  ASYNC: log={:?}, total={:?}, wrote {} msgs (DROPPED {})",
            async_log_time2, async_total_time2, async_count2, LARGE_BATCH - async_count2);
        println!("  Speedup: {:.1}x faster log time", sync_log_time2.as_secs_f64() / async_log_time2.as_secs_f64());
        println!();

        println!("=== Summary ===");
        println!("- Async is faster because log() doesn't wait for disk I/O");
        println!("- But async DROPS messages when queue fills up (current behavior)");
        println!("- Use async for real-time apps where some data loss is acceptable");
        println!("- Use sync for data integrity where every message must be written");
        println!("================================================\n");
    }
}
