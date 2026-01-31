use anyhow::Result;
use foxglove::{websocket::PlaybackStatus, WebSocketServerHandle};
use std::time::Instant;

/// A data source that supports ranged playback with play/pause, seek, and variable speed.
///
/// Implementations are responsible for:
/// - Tracking playback state (playing/paused/ended) and current position
/// - Pacing message delivery according to timestamps and playback speed
/// - Logging messages to channels and broadcasting time updates to the server
pub trait PlaybackSource {
    /// Returns the (start, end) time bounds of the data in nanoseconds.
    ///
    /// Determining this is dependent on the format of data you are loading.
    fn time_range(&self) -> (u64, u64);

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

    /// Seeks to the specified timestamp in nanoseconds.
    ///
    /// Called by a ServerListener when it receives a PlaybackControlRequest from Foxglove
    fn seek(&mut self, log_time: u64) -> Result<()>;

    /// Returns the current playback status.
    ///
    /// Used to send a PlaybackState to Foxglove
    fn status(&self) -> PlaybackStatus;

    /// Returns the current playback position in nanoseconds.
    ///
    /// Used to send a PlaybackState to Foxglove
    fn current_time(&self) -> u64;

    /// Returns the current playback speed multiplier.
    ///
    /// Used to send a PlaybackState to Foxglove
    fn playback_speed(&self) -> f32;

    /// Returns the next wall-clock time at which messages should be logged, and should factor in
    /// playback speed.
    ///
    /// Returns `None` when there are no more messages, indicating playback has ended.
    fn next_wakeup(&mut self) -> Option<Instant>;

    /// Logs pending messages up to the current playback time and broadcasts time updates.
    ///
    /// This should be called after sleeping until `next_wakeup()`. It logs all messages
    /// whose timestamps fall within the elapsed playback time and should call
    /// `server.broadcast_time()` to keep Foxglove's playback bar synchronized.
    fn log_messages(&mut self, server: &WebSocketServerHandle) -> Result<()>;
}
