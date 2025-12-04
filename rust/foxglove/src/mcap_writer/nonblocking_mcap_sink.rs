//! Nonblocking MCAP sink that doesn't block on disk I/O.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::io::{Seek, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use super::mcap_sink::write_message;
use crate::{ChannelDescriptor, FoxgloveError, Metadata, RawChannel, Sink, SinkChannelFilter, SinkId};

/// Default maximum number of messages that can be queued before new messages are dropped.
const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// A queued log message.
struct QueuedLog {
    descriptor: ChannelDescriptor,
    msg: Box<[u8]>,
    metadata: Metadata,
}

/// Commands for the background writer thread.
enum WriteCommand<W> {
    Log(QueuedLog),
    Metadata { name: String, data: BTreeMap<String, String> },
    Finish(mpsc::Sender<Result<W, FoxgloveError>>),
}

/// Background thread that processes queued write commands.
fn run_writer_thread<W: Write + Seek>(
    rx: flume::Receiver<WriteCommand<W>>,
    mut writer: mcap::Writer<W>,
) {
    let mut channel_map = HashMap::new();
    let mut channel_seq = HashMap::new();

    while let Ok(cmd) = rx.recv() {
        match cmd {
            WriteCommand::Log(log) => {
                if let Err(e) = write_message(
                    &mut writer,
                    &mut channel_map,
                    &mut channel_seq,
                    &log.descriptor,
                    &log.msg,
                    &log.metadata,
                ) {
                    tracing::error!("MCAP write error: {e}");
                }
            }
            WriteCommand::Metadata { name, data } => {
                if let Err(e) = writer.write_metadata(&mcap::records::Metadata { name, metadata: data }) {
                    tracing::error!("MCAP metadata write error: {e}");
                }
            }
            WriteCommand::Finish(done) => {
                let result = writer.finish().map_err(FoxgloveError::from);
                let _ = done.send(result.map(|_summary| writer.into_inner()));
                return;
            }
        }
    }
    // Channel closed without Finish - still finalize the file
    let _ = writer.finish();
}

/// An MCAP sink that writes in a background thread.
///
/// Unlike the synchronous `McapSink`, this version queues messages and writes
/// them on a dedicated thread, so `log()` never blocks on disk I/O.
pub(super) struct NonblockingMcapSink<W: Write + Seek + Send + 'static> {
    sink_id: SinkId,
    tx: flume::Sender<WriteCommand<W>>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    /// Tracks whether finish() was already called (to avoid double-finish in Drop)
    finished: AtomicBool,
}

impl<W: Write + Seek + Send + 'static> Debug for NonblockingMcapSink<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NonblockingMcapSink")
            .field("sink_id", &self.sink_id)
            .finish_non_exhaustive()
    }
}

impl<W: Write + Seek + Send + 'static> NonblockingMcapSink<W> {
    /// Creates a new nonblocking sink. Spawns a background thread for writes.
    ///
    /// If `queue_capacity` is `None`, uses the default capacity (1024).
    pub fn new(
        writer: W,
        options: mcap::WriteOptions,
        channel_filter: Option<Arc<dyn SinkChannelFilter>>,
        queue_capacity: Option<usize>,
    ) -> Result<Self, FoxgloveError> {
        let mcap_writer = options.create(writer)?;
        let capacity = queue_capacity.unwrap_or(DEFAULT_QUEUE_CAPACITY);
        let (tx, rx) = flume::bounded::<WriteCommand<W>>(capacity);

        std::thread::spawn(move || {
            run_writer_thread(rx, mcap_writer);
        });

        Ok(Self {
            sink_id: SinkId::next(),
            tx,
            channel_filter,
            finished: AtomicBool::new(false),
        })
    }

    /// Blocks until all queued writes complete, closes the file, and returns the writer.
    /// Called automatically when the sink is dropped.
    pub fn finish(&self) -> Result<W, FoxgloveError> {
        if self.finished.swap(true, Ordering::SeqCst) {
            return Err(FoxgloveError::SinkClosed);
        }

        let (tx, rx) = mpsc::channel();
        self.tx
            .send(WriteCommand::Finish(tx))
            .map_err(|_| FoxgloveError::SinkClosed)?;
        rx.recv().unwrap_or(Err(FoxgloveError::SinkClosed))
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

impl<W: Write + Seek + Send + 'static> Sink for NonblockingMcapSink<W> {
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

impl<W: Write + Seek + Send + 'static> Drop for NonblockingMcapSink<W> {
    fn drop(&mut self) {
        if !self.finished.load(Ordering::SeqCst) {
            if let Err(e) = self.finish() {
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
    #[test]
    fn test_nonblocking_log_channels() {
        let ctx = Context::new();
        let ch1 = new_test_channel(&ctx, "foo", "foo_schema");
        let ch2 = new_test_channel(&ctx, "bar", "bar_schema");

        let temp_file = NamedTempFile::new().expect("create tempfile");
        let temp_path = temp_file.path().to_owned();
        let file = temp_file.reopen().expect("reopen");

        let sink = NonblockingMcapSink::new(file, WriteOptions::default(), None, None)
            .expect("failed to create sink");

        sink.log(&ch1, b"msg1", &Metadata { log_time: 100 }).unwrap();
        sink.log(&ch2, b"msg2", &Metadata { log_time: 200 }).unwrap();
        sink.log(&ch1, b"msg3", &Metadata { log_time: 300 }).unwrap();
        sink.log(&ch2, b"msg4", &Metadata { log_time: 400 }).unwrap();

        sink.finish().expect("failed to finish");

        let messages = read_mcap_messages(&temp_path);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].channel.topic, "foo");
        assert_eq!(messages[0].data.as_ref(), b"msg1");
        assert_eq!(messages[1].channel.topic, "bar");
        assert_eq!(messages[2].channel.topic, "foo");
        assert_eq!(messages[3].channel.topic, "bar");
    }

    /// Tests that dropping the sink without calling finish still produces a valid file.
    #[test]
    fn test_nonblocking_drop_finishes_file() {
        let temp_file = NamedTempFile::new().expect("create tempfile");
        let temp_path = temp_file.path().to_owned();
        let file = temp_file.reopen().expect("reopen");

        let ctx = Context::new();
        let ch = new_test_channel(&ctx, "test", "schema");

        {
            let sink = NonblockingMcapSink::new(file, WriteOptions::default(), None, None)
                .expect("failed to create sink");

            sink.log(&ch, b"drop_test", &Metadata { log_time: 99 }).unwrap();
            // Don't call finish - let Drop handle it
        }

        let messages = read_mcap_messages(&temp_path);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].data.as_ref(), b"drop_test");
    }

    /// Stress test comparing sync vs nonblocking write performance.
    ///
    /// Run with: cargo test -p foxglove stress_test --release -- --ignored --nocapture
    #[test]
    #[ignore]
    fn stress_test_sync_vs_nonblocking() {
        use crate::mcap_writer::mcap_sink::McapSink;
        use std::time::Instant;

        let message = vec![0u8; 1024]; // 1KB message
        let ctx = Context::new();
        let ch = new_test_channel(&ctx, "stress", "schema");

        println!("\n=== Stress Test: Sync vs Nonblocking MCAP Writing ===\n");

        // =====================================================================
        // TEST 1: Small batch (fits in queue) - fair comparison
        // =====================================================================
        const SMALL_BATCH: usize = DEFAULT_QUEUE_CAPACITY / 2; // Less than queue size

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

        let temp_nonblocking = NamedTempFile::new().expect("create tempfile");
        let nonblocking_sink = NonblockingMcapSink::new(
            temp_nonblocking.reopen().expect("reopen"),
            WriteOptions::default(),
            None,
            None,
        ).expect("create nonblocking sink");

        let nonblocking_start = Instant::now();
        for i in 0..SMALL_BATCH {
            nonblocking_sink.log(&ch, &message, &Metadata { log_time: i as u64 }).unwrap();
        }
        let nonblocking_log_time = nonblocking_start.elapsed();
        nonblocking_sink.finish().expect("finish nonblocking");
        let nonblocking_total_time = nonblocking_start.elapsed();
        let nonblocking_count = read_mcap_messages(temp_nonblocking.path()).len();

        println!("TEST 1: Small batch ({} messages x 1KB, fits in queue)", SMALL_BATCH);
        println!("  SYNC:        log={:?}, total={:?}, wrote {} msgs", sync_log_time, sync_total_time, sync_count);
        println!("  NONBLOCKING: log={:?}, total={:?}, wrote {} msgs", nonblocking_log_time, nonblocking_total_time, nonblocking_count);
        println!("  Speedup: {:.1}x faster log time", sync_log_time.as_secs_f64() / nonblocking_log_time.as_secs_f64());
        println!();

        // =====================================================================
        // TEST 2: Large batch (exceeds queue) - shows drop behavior
        // =====================================================================
        const LARGE_BATCH: usize = 10_000; // Larger than DEFAULT_QUEUE_CAPACITY but reasonable

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

        let temp_nonblocking2 = NamedTempFile::new().expect("create tempfile");
        let nonblocking_sink2 = NonblockingMcapSink::new(
            temp_nonblocking2.reopen().expect("reopen"),
            WriteOptions::default(),
            None,
            None,
        ).expect("create nonblocking sink");

        let nonblocking_start2 = Instant::now();
        for i in 0..LARGE_BATCH {
            nonblocking_sink2.log(&ch, &message, &Metadata { log_time: i as u64 }).unwrap();
        }
        let nonblocking_log_time2 = nonblocking_start2.elapsed();
        nonblocking_sink2.finish().expect("finish nonblocking");
        let nonblocking_total_time2 = nonblocking_start2.elapsed();
        let nonblocking_count2 = read_mcap_messages(temp_nonblocking2.path()).len();

        println!("TEST 2: Large batch ({} messages x 1KB, exceeds queue of {})", LARGE_BATCH, DEFAULT_QUEUE_CAPACITY);
        println!("  SYNC:        log={:?}, total={:?}, wrote {} msgs", sync_log_time2, sync_total_time2, sync_count2);
        println!("  NONBLOCKING: log={:?}, total={:?}, wrote {} msgs (DROPPED {})",
            nonblocking_log_time2, nonblocking_total_time2, nonblocking_count2, LARGE_BATCH - nonblocking_count2);
        println!("  Speedup: {:.1}x faster log time", sync_log_time2.as_secs_f64() / nonblocking_log_time2.as_secs_f64());
        println!();

        println!("=== Summary ===");
        println!("- Nonblocking is faster because log() doesn't wait for disk I/O");
        println!("- But nonblocking DROPS messages when queue fills up (current behavior)");
        println!("- Use nonblocking for real-time apps where some data loss is acceptable");
        println!("- Use sync for data integrity where every message must be written");
        println!("================================================\n");
    }
}
