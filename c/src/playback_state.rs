use crate::FoxgloveString;

#[repr(C)]
pub struct FoxglovePlaybackState {
    /// The status of server data playback
    pub status: u8,
    /// The current time of playback, in absolute nanoseconds
    pub current_time: u64,
    /// The speed of playback, as a factor of realtime
    pub playback_speed: f32,
    /// If this message is being emitted in response to a PlaybackControlRequest message, the
    /// request_id from that message. Set this to an empty string if the state of playback has been changed
    /// by any other condition.
    pub request_id: FoxgloveString,
}

impl FoxglovePlaybackState {
    /// Create a native playback state from a FoxglovePlaybackState.
    ///
    /// # Safety
    /// - `self.request_id` must wrap a valid UTF-8 string that lives for the duration of the call.
    ///   This function copies request_id into a separately allocated Rust string in the output native PlaybackState.
    pub(crate) fn as_native(
        &self,
    ) -> Result<foxglove::websocket::PlaybackState, foxglove::FoxgloveError> {
        let status = foxglove::websocket::PlaybackStatus::try_from(self.status).map_err(|e| {
            foxglove::FoxgloveError::ValueError(format!("invalid playback status {e}"))
        })?;

        let request_id = if self.request_id.is_empty() {
            None
        } else {
            Some(unsafe { self.request_id.as_utf8_str()? }.to_string())
        };

        Ok(foxglove::websocket::PlaybackState {
            status,
            playback_speed: self.playback_speed,
            current_time: self.current_time,
            request_id,
        })
    }
}
