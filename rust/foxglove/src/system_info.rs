//! Optional publisher that reports process and system statistics.
//!
//! Build a [`SystemInfoPublisher`] and call [`SystemInfoPublisher::start`]
//! to spawn a background task that periodically logs a SystemInfo
//! message to a channel. The default channel is `/sysinfo`, and the default
//! refresh interval is 500ms.
//!
//! The returned [`SystemInfoHandle`] can be `.await`ed to wait for the
//! publisher to complete, or aborted with [`SystemInfoHandle::abort`].

use std::borrow::Cow;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::Duration;

use bytes::BufMut;
use serde::Serialize;
use sysinfo::{
    CpuRefreshKind, MINIMUM_CPU_UPDATE_INTERVAL, Pid, ProcessRefreshKind, ProcessesToUpdate, System,
};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::{Channel, ChannelBuilder, Context, Encode, Schema, runtime::get_runtime_handle};

/// The default topic the [`SystemInfoPublisher`] publishes to.
pub const DEFAULT_SYSINFO_TOPIC: &str = "/sysinfo";

/// The default refresh interval for [`SystemInfoPublisher`].
pub const DEFAULT_SYSINFO_REFRESH_INTERVAL: Duration = Duration::from_millis(500);

/// JSON Schema (draft 2020-12) describing SystemInfo for consumers of the `/sysinfo` topic.
const SYSINFO_JSON_SCHEMA: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "SystemInfo",
  "description": "A snapshot of process and system statistics published on the /sysinfo topic.",
  "type": "object",
  "properties": {
    "process_memory": {
      "type": "number",
      "description": "Resident memory used by the SDK process, in bytes."
    },
    "process_virtual_memory": {
      "type": "number",
      "description": "Virtual memory used by the SDK process, in bytes."
    },
    "process_cpu_percent": {
      "type": "number",
      "description": "CPU usage for the SDK process, as a percent of total system CPU capacity (0.0 to 100.0). 100.0 means the process is using all logical CPUs at full utilization."
    },
    "process_cpu_cores": {
      "type": "number",
      "description": "CPU usage for the SDK process, expressed in core-equivalents (0.0 to num_cpus). 1.0 means a single logical CPU is fully utilized."
    },
    "total_cpu_percent": {
      "type": "number",
      "description": "Total CPU usage across all logical CPUs on the system, as a percent (0.0 to 100.0)."
    },
    "total_cpu_cores": {
      "type": "number",
      "description": "Total CPU usage across the system, expressed in core-equivalents (0.0 to num_cpus). 1.0 means one logical CPU's worth of work is being done."
    },
    "num_cpus": {
      "type": "integer",
      "minimum": 0,
      "description": "Number of logical CPUs on the system."
    },
    "total_memory": {
      "type": "number",
      "description": "Total physical memory on the system, in bytes."
    },
    "used_memory": {
      "type": "number",
      "description": "Used physical memory on the system, in bytes."
    },
    "total_swap": {
      "type": "number",
      "description": "Total swap space on the system, in bytes."
    },
    "used_swap": {
      "type": "number",
      "description": "Used swap space on the system, in bytes."
    },
    "kernel_version": {
      "type": "string",
      "description": "Kernel version string, or empty if unavailable on this platform."
    },
    "os_version": {
      "type": "string",
      "description": "OS version string, or empty if unavailable on this platform."
    }
  }
}"#;

/// A snapshot of process and system statistics published by [`SystemInfoPublisher`].
///
/// Encoded as JSON on the wire, with a JSON Schema attached to the channel.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct SystemInfo {
    /// Resident memory used by the SDK process, in bytes.
    pub process_memory: f64,
    /// Virtual memory used by the SDK process, in bytes.
    pub process_virtual_memory: f64,
    /// CPU usage for the SDK process, as a percent of total system CPU capacity
    /// (0.0 to 100.0).
    ///
    /// 100.0 means the process is using all logical CPUs at full utilization.
    pub process_cpu_percent: f64,
    /// CPU usage for the SDK process, expressed in core-equivalents
    /// (0.0 to `num_cpus`).
    ///
    /// 1.0 means a single logical CPU is fully utilized.
    pub process_cpu_cores: f64,
    /// Total CPU usage across all logical CPUs on the system, as a percent (0.0 to 100.0).
    pub total_cpu_percent: f64,
    /// Total CPU usage across the system, expressed in core-equivalents
    /// (0.0 to `num_cpus`).
    ///
    /// 1.0 means one logical CPU's worth of work is being done.
    pub total_cpu_cores: f64,
    /// Number of logical CPUs on the system.
    pub num_cpus: u32,
    /// Total physical memory on the system, in bytes.
    pub total_memory: f64,
    /// Used physical memory on the system, in bytes.
    pub used_memory: f64,
    /// Total swap space on the system, in bytes.
    pub total_swap: f64,
    /// Used swap space on the system, in bytes.
    pub used_swap: f64,
    /// Kernel version string, or empty if unavailable on this platform.
    pub kernel_version: String,
    /// OS version string, or empty if unavailable on this platform.
    pub os_version: String,
}

impl Encode for SystemInfo {
    type Error = serde_json::Error;

    fn get_schema() -> Option<Schema> {
        Some(Schema::new(
            "foxglove.SystemInfo".to_string(),
            "jsonschema".to_string(),
            Cow::Borrowed(SYSINFO_JSON_SCHEMA.as_bytes()),
        ))
    }

    fn get_message_encoding() -> String {
        "json".to_string()
    }

    fn encode(&self, buf: &mut impl BufMut) -> Result<(), Self::Error> {
        serde_json::to_writer(buf.writer(), self)
    }
}

/// Builder for the system info publisher.
///
/// The publisher creates a channel on the configured [`Context`] and spawns a
/// background task that periodically logs a SystemInfo message to the channel
/// with stats on CPU and memory usage for the process and the system.
///
/// # Example
///
/// ```no_run
/// use std::time::Duration;
/// use foxglove::system_info::SystemInfoPublisher;
///
/// # async fn run() {
/// let handle = SystemInfoPublisher::new()
///     .refresh_interval(Duration::from_secs(2))
///     .start();
/// // ... do other work ...
/// handle.abort();
/// # }
/// ```
#[must_use]
#[derive(Debug, Default)]
pub struct SystemInfoPublisher {
    topic: Option<String>,
    refresh_interval: Option<Duration>,
    context: Option<Arc<Context>>,
}

impl SystemInfoPublisher {
    /// Creates a new publisher builder with default settings.
    ///
    /// The defaults are:
    /// - topic: [`DEFAULT_SYSINFO_TOPIC`] (`/sysinfo`)
    /// - refresh interval: [`DEFAULT_SYSINFO_REFRESH_INTERVAL`] (500ms)
    /// - context: the global default context
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the channel topic name.
    ///
    /// Defaults to [`DEFAULT_SYSINFO_TOPIC`].
    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Sets the refresh interval.
    ///
    /// The interval is clamped to a minimum of 200ms because CPU usage
    /// cannot be reliably computed for shorter intervals.
    ///
    /// Defaults to [`DEFAULT_SYSINFO_REFRESH_INTERVAL`].
    pub fn refresh_interval(mut self, interval: Duration) -> Self {
        self.refresh_interval = Some(interval);
        self
    }

    /// Sets the [`Context`] on which the publisher creates its channel.
    ///
    /// Defaults to the global default context.
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = Some(ctx.clone());
        self
    }

    /// Starts the publisher and returns a [`SystemInfoHandle`] for the background task.
    ///
    /// The task is intended to run until [`SystemInfoHandle::abort`] is called on the
    /// returned handle.
    ///
    /// The publisher creates its channel and registers it with the context
    /// synchronously before spawning the background task. The channel is
    /// closed when the background task exits (for example after `abort`).
    pub fn start(self) -> SystemInfoHandle {
        // If we refresh too quickly we'll get invalid values for cpu usage.
        // sysinfo crate exports a platform dependent MINIMUM_CPU_UPDATE_INTERVAL
        // that is 200ms on Linux. However, it is 0 on unknown platforms.
        // We clamp to 200ms as well to be safe.
        let requested_interval = self
            .refresh_interval
            .unwrap_or(DEFAULT_SYSINFO_REFRESH_INTERVAL);

        let min_interval = MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(200));

        let refresh_interval = if requested_interval < min_interval {
            tracing::info!(
                "sysinfo refresh interval {requested_interval:?} is below minimum {min_interval:?}, clamping"
            );
            min_interval
        } else {
            requested_interval
        };

        let topic = self
            .topic
            .unwrap_or_else(|| DEFAULT_SYSINFO_TOPIC.to_string());
        let context = self.context.unwrap_or_else(Context::get_default);

        // Create the channel synchronously so it's registered before start() returns.
        let channel = ChannelBuilder::new(topic)
            .context(&context)
            .build::<SystemInfo>();

        SystemInfoHandle {
            inner: get_runtime_handle().spawn(run_publisher(channel, refresh_interval)),
        }
    }
}

/// Handle to a running [`SystemInfoPublisher`] background task.
///
/// Returned by [`SystemInfoPublisher::start`]. The handle can be `.await`ed to
/// wait for the publisher to finish, or [`abort`](Self::abort)ed to stop it.
/// Dropping the handle does not stop the publisher; it will continue running
/// until [`abort`](Self::abort) is called.
#[must_use = "the publisher keeps running until aborted; the handle is the only way to wait for or abort it"]
#[derive(Debug)]
pub struct SystemInfoHandle {
    inner: JoinHandle<()>,
}

impl SystemInfoHandle {
    /// Aborts the publisher's background task.
    ///
    /// The task is signaled to stop at the next `.await` point. After calling
    /// this, awaiting the handle will resolve once the task has actually
    /// terminated.
    pub fn abort(&self) {
        self.inner.abort();
    }
}

impl Future for SystemInfoHandle {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        // The publisher task itself never panics or returns an error, and we
        // intentionally swallow JoinError (e.g. from abort) so that this
        // handle does not expose tokio types in its public API.
        Pin::new(&mut self.inner).poll(cx).map(|_| ())
    }
}

async fn run_publisher(channel: Channel<SystemInfo>, refresh_interval: Duration) {
    let pid = Pid::from_u32(std::process::id());
    let kernel_version = System::kernel_version().unwrap_or_default();
    let os_version = System::os_version().unwrap_or_default();

    let mut system = System::new();
    // Populate the CPU list once so that cpus().len() returns the correct count.
    // New CPUs being added at runtime is vanishingly rare, so we don't refresh this.
    let cpu_refresh = CpuRefreshKind::nothing().with_cpu_usage();
    system.refresh_cpu_list(cpu_refresh);
    let num_cpus = u32::try_from(system.cpus().len()).unwrap_or(u32::MAX);

    let process_refresh = ProcessRefreshKind::nothing().with_cpu().with_memory();
    // Prime the per-process and global CPU usage; the first reading is always 0
    // since it relies on diffs between consecutive samples.
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, process_refresh);
    system.refresh_cpu_specifics(cpu_refresh);

    let mut interval = tokio::time::interval(refresh_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // The first tick fires immediately; consume it to align subsequent ticks to the period.
    interval.tick().await;

    loop {
        interval.tick().await;

        system.refresh_memory();
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, process_refresh);
        system.refresh_cpu_specifics(cpu_refresh);

        let (process_memory, process_virtual_memory, process_cpu_per_core_percent) = system
            .process(pid)
            .map(|p| (p.memory(), p.virtual_memory(), p.cpu_usage()))
            .unwrap_or_default();

        // sysinfo reports per-process CPU usage normalized per logical CPU
        // (100.0 == one CPU fully utilized, max == 100.0 * num_cpus), and global
        // CPU usage as a 0-100 percent of total capacity. Convert both to a
        // common pair of representations: a percent of total system capacity
        // and an equivalent number of busy cores.
        let process_cpu_cores = f64::from(process_cpu_per_core_percent) / 100.0;
        let process_cpu_percent = if num_cpus > 0 {
            f64::from(process_cpu_per_core_percent) / f64::from(num_cpus)
        } else {
            0.0
        };
        let total_cpu_percent = f64::from(system.global_cpu_usage());
        let total_cpu_cores = total_cpu_percent * f64::from(num_cpus) / 100.0;

        let info = SystemInfo {
            process_memory: process_memory as f64,
            process_virtual_memory: process_virtual_memory as f64,
            process_cpu_percent,
            process_cpu_cores,
            total_cpu_percent,
            total_cpu_cores,
            num_cpus,
            total_memory: system.total_memory() as f64,
            used_memory: system.used_memory() as f64,
            total_swap: system.total_swap() as f64,
            used_swap: system.used_swap() as f64,
            kernel_version: kernel_version.clone(),
            os_version: os_version.clone(),
        };

        channel.log(&info);
    }
}

#[cfg(test)]
mod tests {
    use crate::{Context, Encode};

    use super::*;

    #[test]
    fn schema_is_jsonschema() {
        assert_eq!(SystemInfo::get_message_encoding(), "json");
        let schema = SystemInfo::get_schema().expect("SystemInfo has a schema");
        assert_eq!(schema.encoding, "jsonschema");
        let parsed: serde_json::Value =
            serde_json::from_slice(&schema.data).expect("schema must be valid JSON");
        assert_eq!(parsed["type"], "object");
        assert!(parsed["properties"]["process_memory"].is_object());
    }

    #[test]
    fn schema_properties_match_serialized_fields() {
        // Guards against drift between the hand-written JSON schema and the
        // SystemInfo struct: any field added/renamed/removed must be reflected
        // in both places, otherwise this test fails.
        let info = SystemInfo {
            process_memory: 0.0,
            process_virtual_memory: 0.0,
            process_cpu_percent: 0.0,
            process_cpu_cores: 0.0,
            total_cpu_percent: 0.0,
            total_cpu_cores: 0.0,
            num_cpus: 0,
            total_memory: 0.0,
            used_memory: 0.0,
            total_swap: 0.0,
            used_swap: 0.0,
            kernel_version: String::new(),
            os_version: String::new(),
        };
        let serialized: serde_json::Value =
            serde_json::to_value(&info).expect("SystemInfo serializes");
        let serialized_keys: std::collections::BTreeSet<String> = serialized
            .as_object()
            .expect("SystemInfo serializes as a JSON object")
            .keys()
            .cloned()
            .collect();

        let schema: serde_json::Value =
            serde_json::from_str(SYSINFO_JSON_SCHEMA).expect("schema is valid JSON");
        let schema_keys: std::collections::BTreeSet<String> = schema["properties"]
            .as_object()
            .expect("schema has a properties object")
            .keys()
            .cloned()
            .collect();

        assert_eq!(
            schema_keys, serialized_keys,
            "JSON schema properties must match SystemInfo serialized field names"
        );
    }

    #[test]
    fn encodes_as_json() {
        let info = SystemInfo {
            process_memory: 1.0,
            process_virtual_memory: 2.0,
            process_cpu_percent: 3.0,
            process_cpu_cores: 3.5,
            total_cpu_percent: 4.0,
            total_cpu_cores: 4.5,
            num_cpus: 5,
            total_memory: 6.0,
            used_memory: 7.0,
            total_swap: 8.0,
            used_swap: 9.0,
            kernel_version: "k".to_string(),
            os_version: "o".to_string(),
        };
        let mut buf = Vec::new();
        info.encode(&mut buf).expect("encode");
        let parsed: serde_json::Value = serde_json::from_slice(&buf).expect("valid JSON");
        assert_eq!(parsed["num_cpus"], 5);
        assert_eq!(parsed["kernel_version"], "k");
        assert_eq!(parsed["os_version"], "o");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn publisher_exits_when_aborted() {
        let ctx = Context::new();
        let handle = SystemInfoPublisher::new()
            .context(&ctx)
            .refresh_interval(Duration::from_millis(200))
            .start();

        // Let the task reach the select loop (past the priming calls).
        tokio::task::yield_now().await;
        handle.abort();
        // The handle resolves once the task has actually stopped; bound the wait
        // so we don't hang the test if abort is not honored.
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("publisher should exit after abort");
    }

    #[test]
    fn publisher_uses_default_topic_and_context() {
        let publisher = SystemInfoPublisher::new();
        assert!(publisher.topic.is_none());
        assert!(publisher.refresh_interval.is_none());
        assert!(publisher.context.is_none());
    }

    #[test]
    fn publisher_can_override_topic() {
        let publisher = SystemInfoPublisher::new().topic("/custom/sysinfo");
        assert_eq!(publisher.topic.as_deref(), Some("/custom/sysinfo"));
    }
}
