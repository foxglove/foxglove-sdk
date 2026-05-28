use std::time::Duration;

use anyhow::Result;
use foxglove::{WebSocketServerHandle, websocket::PlaybackStatus};

/// A timestamp in nanoseconds since epoch.
pub type Nanoseconds = u64;

/// A data source that supports playback control with play/pause, seek, and variable speed.
///
/// Implementations are responsible for:
/// - Tracking playback state (playing/paused/ended) and current position
/// - Pacing message delivery according to timestamps and playback speed
/// - Logging messages to channels and broadcasting time updates to the server
pub trait PlaybackSource {
    /// Returns the (start, end) time bounds of the data.
    ///
    /// Determining this is dependent on the format of data you are loading.
    fn time_range(&self) -> (Nanoseconds, Nanoseconds);

    /// Sets the playback speed multiplier (e.g., 1.0 for real-time, 2.0 for double speed).
    ///
    /// Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove
    fn set_playback_speed(&mut self, speed: f32);

    /// Begins or resumes playback.
    ///
    /// Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove
    fn play(&mut self);

    /// Pauses playback.
    ///
    /// Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove
    fn pause(&mut self);

    /// Seeks to the specified timestamp.
    ///
    /// Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove
    fn seek(&mut self, log_time: Nanoseconds) -> Result<()>;

    /// Returns the current playback status.
    ///
    /// Used to send a PlaybackState to Foxglove
    fn status(&self) -> PlaybackStatus;

    /// Returns the current playback position.
    ///
    /// Used to send a PlaybackState to Foxglove
    fn current_time(&self) -> Nanoseconds;

    /// Returns the current playback speed multiplier.
    ///
    /// Used to send a PlaybackState to Foxglove
    fn playback_speed(&self) -> f32;

    /// Logs the next message for playback if it's ready, or returns a wall duration to wait
    /// (accounting for the current playback speed). This should be called by your main playback
    /// loop.
    ///
    /// Returns `Ok(Some(duration))` if the caller should sleep before calling again.
    /// Returns `Ok(None)` if a message was logged or playback is not active.
    ///
    /// The caller should sleep outside of any lock to allow control requests to be processed.
    /// This method also broadcasts time updates via `server.broadcast_time()`.
    fn log_next_message(&mut self, server: &WebSocketServerHandle) -> Result<Option<Duration>>;
}
