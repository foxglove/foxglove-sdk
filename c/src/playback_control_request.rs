use crate::FoxgloveString;

#[repr(C)]
pub struct FoxglovePlaybackControlRequest<'a> {
    /// Playback command
    pub playback_command: u8,
    /// Playback speed
    pub playback_speed: f32,
    /// Seek playback time in nanoseconds (only set if a seek has been performed)
    pub seek_time: Option<&'a u64>,
    /// Unique string identifier, used to indicate that a PlaybackState is in response to a particular request from the client.
    /// Should not be an empty string.
    pub request_id: FoxgloveString,
}
