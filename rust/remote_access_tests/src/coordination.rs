// File-based done signal between gateway and viewer containers for per-link
// netem tests. The viewer writes a done signal when it finishes receiving
// messages so the gateway knows it can shut down. Both sides share a tmpfs
// volume at COORDINATION_DIR.
//
// The room name is passed via the PERLINK_ROOM_NAME environment variable
// (set by the orchestrating script or CI workflow).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result};
use tracing::info;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

const DONE_FILE: &str = "done.txt";

/// Returns the coordination directory from the `COORDINATION_DIR` env var.
fn coordination_dir() -> Result<PathBuf> {
    let dir = std::env::var("COORDINATION_DIR")
        .context("COORDINATION_DIR not set — are you running inside a perlink container?")?;
    Ok(PathBuf::from(dir))
}

/// Returns the room name from the `PERLINK_ROOM_NAME` env var, set by the
/// orchestrating script (or CI workflow).
pub fn room_name() -> Result<String> {
    std::env::var("PERLINK_ROOM_NAME")
        .context("PERLINK_ROOM_NAME not set — are you running via the perlink orchestration?")
}

/// Signals completion by writing `done.txt`. The file's presence is the
/// signal; its contents are not checked by [`poll_done`].
pub fn write_done() -> Result<()> {
    let path = coordination_dir()?.join(DONE_FILE);
    std::fs::write(&path, "done")
        .with_context(|| format!("failed to write done signal to {}", path.display()))?;
    info!("wrote done signal to {}", path.display());
    Ok(())
}

/// Gateway polls until `done.txt` appears.
pub async fn poll_done(timeout: Duration) -> Result<()> {
    let path = coordination_dir()?.join(DONE_FILE);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if path.exists() {
            info!("done signal received at {}", path.display());
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timeout ({timeout:?}) waiting for done signal at {}",
                path.display()
            );
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Remove coordination files. Call at the start of each test run.
pub fn clean() -> Result<()> {
    let dir = coordination_dir()?;
    let path = dir.join(DONE_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("failed to remove {}", path.display()));
        }
    }
    info!("cleaned coordination dir {}", dir.display());
    Ok(())
}
