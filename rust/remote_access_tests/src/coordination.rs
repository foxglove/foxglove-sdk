// File-based coordination between gateway and viewer containers for per-link
// netem tests. The gateway writes a room name for the viewer to discover, and
// the viewer writes a done signal when it finishes. Both sides poll the shared
// COORDINATION_DIR (a tmpfs volume mounted in both containers).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result};
use tracing::info;

const POLL_INTERVAL: Duration = Duration::from_millis(200);

const ROOM_NAME_FILE: &str = "room-name.txt";
const DONE_FILE: &str = "done.txt";

/// Returns the coordination directory from the `COORDINATION_DIR` env var.
pub fn coordination_dir() -> Result<PathBuf> {
    let dir = std::env::var("COORDINATION_DIR")
        .context("COORDINATION_DIR not set — are you running inside a perlink container?")?;
    Ok(PathBuf::from(dir))
}

/// Gateway writes a room name for the viewer to discover.
pub fn write_room_name(room_name: &str) -> Result<()> {
    let path = coordination_dir()?.join(ROOM_NAME_FILE);
    std::fs::write(&path, room_name)
        .with_context(|| format!("failed to write room name to {}", path.display()))?;
    info!("wrote room name to {}", path.display());
    Ok(())
}

/// Viewer polls until `room-name.txt` appears and returns its contents.
pub async fn poll_room_name(timeout: Duration) -> Result<String> {
    let path = coordination_dir()?.join(ROOM_NAME_FILE);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            let name = contents.trim().to_string();
            if !name.is_empty() {
                info!("read room name from {}: {name}", path.display());
                return Ok(name);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timeout ({timeout:?}) waiting for room name at {}",
                path.display()
            );
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
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
    for name in [ROOM_NAME_FILE, DONE_FILE] {
        let path = dir.join(name);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e).with_context(|| format!("failed to remove {}", path.display()));
            }
        }
    }
    info!("cleaned coordination dir {}", dir.display());
    Ok(())
}
