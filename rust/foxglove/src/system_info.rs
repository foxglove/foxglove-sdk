//! Optional publisher that reports process and system statistics on the
//! `/sysinfo` topic.
//!
//! Enabled per-server via [`WebSocketServer::sysinfo`](crate::WebSocketServer::sysinfo)
//! and [`Gateway::sysinfo`](crate::remote_access::Gateway::sysinfo), and gated behind
//! the `remote-access` or `websocket` feature flags.

use std::borrow::Cow;
use std::future::Future;
use std::sync::Weak;
use std::time::Duration;

use bytes::BufMut;
use serde::Serialize;
use sysinfo::{
    CpuRefreshKind, MINIMUM_CPU_UPDATE_INTERVAL, Pid, ProcessRefreshKind, ProcessesToUpdate, System,
};
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::{ChannelBuilder, Context, Encode, Schema};

const SYSINFO_TOPIC: &str = "/sysinfo";

/// JSON Schema (draft 2020-12) describing [`SystemInfo`] for consumers of the `/sysinfo` topic.
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
      "description": "CPU usage for the SDK process, as a percent. Values are normalized per logical CPU: 100.0 means a single CPU is fully utilized, so the maximum value is 100.0 * num_cpus."
    },
    "total_cpu_percent": {
      "type": "number",
      "description": "Total CPU usage across all logical CPUs on the system, as a percent (0.0 to 100.0)."
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
  },
  "required": [
    "process_memory",
    "process_virtual_memory",
    "process_cpu_percent",
    "total_cpu_percent",
    "num_cpus",
    "total_memory",
    "used_memory",
    "total_swap",
    "used_swap",
    "kernel_version",
    "os_version"
  ]
}"#;

/// A snapshot of process and system statistics published on the `/sysinfo` topic.
///
/// Encoded as JSON on the wire, with a JSON Schema attached to the channel.
#[derive(Clone, Debug, Serialize)]
pub struct SystemInfo {
    /// Resident memory used by the SDK process, in bytes.
    pub process_memory: f64,
    /// Virtual memory used by the SDK process, in bytes.
    pub process_virtual_memory: f64,
    /// CPU usage for the SDK process, as a percent.
    ///
    /// Values are normalized per logical CPU: 100.0 means a single CPU is fully
    /// utilized, so the maximum value is `100.0 * num_cpus`.
    pub process_cpu_percent: f64,
    /// Total CPU usage across all logical CPUs on the system, as a percent (0.0 to 100.0).
    pub total_cpu_percent: f64,
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

/// Returns a future that refreshes system info at the requested interval and
/// publishes it to the `/sysinfo` topic on the provided context.
///
/// `refresh_interval` is clamped to a minimum of 200ms.
///
/// The future completes when `cancel` is triggered, or when the context is dropped.
pub(crate) fn publisher_future(
    context: Weak<Context>,
    cancel: CancellationToken,
    refresh_interval: Duration,
) -> impl Future<Output = ()> + Send + 'static {
    // If we refresh too quickly we'll get invalid values for cpu usage.
    // sysinfo crate exports a platform dependent MINIMUM_CPU_UPDATE_INTERVAL
    // that is 200ms on Linux. However, it is 0 on unknown platforms.
    // We clamp to 200ms as well to be safe.
    let refresh_interval = refresh_interval
        .max(MINIMUM_CPU_UPDATE_INTERVAL)
        .max(Duration::from_millis(200));
    run_publisher(context, cancel, refresh_interval)
}

async fn run_publisher(
    context: Weak<Context>,
    cancel: CancellationToken,
    refresh_interval: Duration,
) {
    let Some(ctx) = context.upgrade() else {
        return;
    };
    let channel = ChannelBuilder::new(SYSINFO_TOPIC)
        .context(&ctx)
        .build::<SystemInfo>();
    // We don't need the context anymore, don't keep it alive longer than needed
    drop(ctx);
    drop(context);

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
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = interval.tick() => {}
        }

        system.refresh_memory();
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, process_refresh);
        system.refresh_cpu_specifics(cpu_refresh);

        let (process_memory, process_virtual_memory, process_cpu_percent) = system
            .process(pid)
            .map(|p| (p.memory(), p.virtual_memory(), p.cpu_usage()))
            .unwrap_or_default();

        let info = SystemInfo {
            process_memory: process_memory as f64,
            process_virtual_memory: process_virtual_memory as f64,
            process_cpu_percent: process_cpu_percent as f64,
            total_cpu_percent: system.global_cpu_usage() as f64,
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
    use std::sync::Arc;

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
    fn encodes_as_json() {
        let info = SystemInfo {
            process_memory: 1.0,
            process_virtual_memory: 2.0,
            process_cpu_percent: 3.0,
            total_cpu_percent: 4.0,
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
    async fn publisher_exits_when_cancelled() {
        let ctx = Context::new();
        let cancel = CancellationToken::new();
        let handle = tokio::spawn(publisher_future(
            Arc::downgrade(&ctx),
            cancel.clone(),
            Duration::from_millis(200),
        ));

        // Let the task reach the select loop (past the priming calls).
        tokio::task::yield_now().await;
        cancel.cancel();
        handle.await.expect("publisher task should exit cleanly");
    }
}
