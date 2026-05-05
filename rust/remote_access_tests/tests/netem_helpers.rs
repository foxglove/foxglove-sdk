//! Shared helpers for netem test suites: argument parsing and container control.

#![allow(dead_code)]

use std::process::Command;

use anyhow::{Context as _, Result};
use tracing::info;

/// Default netem arguments matching `docker-compose.netem.yml`.
const DEFAULT_NETEM_ARGS: &str = "delay 80ms 20ms loss 2%";

/// Return the default netem args, preferring the `NETEM_ARGS` env var.
pub fn default_netem_args() -> String {
    std::env::var("NETEM_ARGS").unwrap_or_else(|_| DEFAULT_NETEM_ARGS.into())
}

/// Find the netem sidecar container ID.
///
/// Uses `docker ps` with a name filter instead of `docker compose ps` to avoid
/// compose-file-path and project-name resolution issues when the test binary's
/// working directory differs from where compose was invoked.
pub fn netem_container_id() -> Result<String> {
    let output = Command::new("docker")
        .args([
            "ps",
            "-q",
            "--filter",
            "name=-netem-[0-9]+$",
            "--filter",
            "status=running",
        ])
        .output()
        .context("failed to run docker ps")?;

    anyhow::ensure!(
        output.status.success(),
        "docker ps failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).context("invalid UTF-8 from docker ps")?;
    let id = stdout.lines().next().unwrap_or("").trim().to_string();

    anyhow::ensure!(
        !id.is_empty(),
        "no running netem container found — is the netem stack running? \
         Start with: docker compose -f docker-compose.yaml \
         -f docker-compose.netem.yml up -d --wait"
    );
    Ok(id)
}

/// Update netem impairment parameters on the given class. Runs
/// `netem_impair.py <class> <args>` inside the netem sidecar.
pub fn set_netem_impairment(container: &str, class: &str, args: &str) -> Result<()> {
    let output = Command::new("docker")
        .args(["exec", container, "python3", "/netem_impair.py", class])
        .args(args.split_whitespace())
        .output()
        .context("failed to run netem_impair.py")?;

    anyhow::ensure!(
        output.status.success(),
        "netem_impair.py failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    info!(
        "netem impairment updated: {}",
        String::from_utf8_lossy(&output.stdout).trim()
    );
    Ok(())
}

/// Parse the base delay (in ms) from a netem args string.
///
/// Matches the first `delay <N>ms` token pair. Returns `None` if no delay is
/// configured or the value cannot be parsed.
///
/// ```text
/// "delay 200ms 50ms loss 5%" → Some(200)
/// "loss 5%"                  → None
/// ```
pub fn parse_delay_ms(netem_args: &str) -> Option<u64> {
    netem_args
        .split_whitespace()
        .zip(netem_args.split_whitespace().skip(1))
        .find(|(key, _)| *key == "delay")
        .and_then(|(_, val)| val.strip_suffix("ms")?.parse().ok())
}

/// Parse the loss percentage from a netem args string.
///
/// Matches the first `loss <N>%` token pair. Returns `None` if no loss is
/// configured or the value cannot be parsed.
///
/// ```text
/// "delay 200ms 50ms loss 5%" → Some(5.0)
/// "delay 10ms 2ms"           → None
/// ```
pub fn parse_loss_percentage(netem_args: &str) -> Option<f64> {
    netem_args
        .split_whitespace()
        .zip(netem_args.split_whitespace().skip(1))
        .find(|(key, _)| *key == "loss")
        .and_then(|(_, val)| val.strip_suffix('%')?.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delay_basic() {
        assert_eq!(parse_delay_ms("delay 200ms 50ms loss 5%"), Some(200));
        assert_eq!(parse_delay_ms("delay 10ms 2ms"), Some(10));
        assert_eq!(parse_delay_ms("loss 5%"), None);
        assert_eq!(parse_delay_ms(""), None);
    }

    #[test]
    fn parse_loss_basic() {
        assert_eq!(parse_loss_percentage("delay 200ms 50ms loss 5%"), Some(5.0));
        assert_eq!(parse_loss_percentage("delay 10ms 2ms"), None);
        assert_eq!(parse_loss_percentage("loss 0.1%"), Some(0.1));
        assert_eq!(parse_loss_percentage(""), None);
    }
}
