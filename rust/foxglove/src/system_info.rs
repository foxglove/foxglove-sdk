//! Optional publisher that reports process and system statistics on the
//! `/sysinfo` topic.
//!
//! Enabled per-server via [`WebSocketServer::sysinfo`](crate::WebSocketServer::sysinfo)
//! and [`Gateway::sysinfo`](crate::remote_access::Gateway::sysinfo), and gated behind
//! the crate's `sysinfo` feature flag.

use std::sync::Weak;
use std::time::Duration;

use sysinfo::{CpuRefreshKind, MINIMUM_CPU_UPDATE_INTERVAL, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::{ChannelBuilder, Context};

const SYSINFO_TOPIC: &str = "/sysinfo";

/// A snapshot of process and system statistics published on the `/sysinfo` topic.
#[derive(Clone, Debug, foxglove_derive::Encode)]
pub struct SystemInfo {
    /// Resident memory used by the SDK process, in bytes.
    pub process_memory: f64,
    /// Virtual memory used by the SDK process, in bytes.
    pub process_virtual_memory: f64,
    /// CPU usage for the SDK process, as a percent.
    ///
    /// Values are normalized per logical CPU: 100.0 means a single CPU is fully
    /// utilized, so the maximum value is `100.0 * num_cpus`.
    pub process_cpu_percent: f32,
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

/// Spawns a background task that refreshes system info at the requested
/// interval and publishes it to the `/sysinfo` topic on the provided context.
///
/// `refresh_interval` is clamped to a minimum of
/// [`sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`], since CPU usage samples taken more
/// frequently than that are not refreshed by the underlying crate.
///
/// The task exits when `cancel` is triggered, or when the context is dropped.
pub(crate) fn spawn_publisher(
    context: Weak<Context>,
    runtime: &Handle,
    cancel: CancellationToken,
    refresh_interval: Duration,
) -> JoinHandle<()> {
    let refresh_interval = refresh_interval.max(MINIMUM_CPU_UPDATE_INTERVAL);
    runtime.spawn(run_publisher(context, cancel, refresh_interval))
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
    // Don't hold a strong reference to the context: if it's dropped externally,
    // the Weak upgrade below will fail and we'll exit cleanly.
    drop(ctx);

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

        // Exit cleanly if the context has been dropped while we were idle.
        if context.strong_count() == 0 {
            break;
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
            process_cpu_percent,
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
    fn schema_is_protobuf() {
        assert_eq!(SystemInfo::get_message_encoding(), "protobuf");
        let schema = SystemInfo::get_schema().expect("SystemInfo has a schema");
        assert_eq!(schema.encoding, "protobuf");
        assert!(!schema.data.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn publisher_exits_when_cancelled() {
        let ctx = Context::new();
        let cancel = CancellationToken::new();
        let handle = spawn_publisher(
            Arc::downgrade(&ctx),
            &tokio::runtime::Handle::current(),
            cancel.clone(),
            Duration::from_millis(200),
        );

        // Let the task reach the select loop (past the priming calls).
        tokio::task::yield_now().await;
        cancel.cancel();
        handle.await.expect("publisher task should exit cleanly");
    }
}
