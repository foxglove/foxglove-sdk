use crate::{FoxgloveError, FoxgloveString, FoxgloveStringBuf};

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
    pub request_id: FoxgloveStringBuf,
}

impl FoxglovePlaybackState {
    pub(crate) fn into_native(
        self,
    ) -> Result<foxglove::websocket::PlaybackState, foxglove::FoxgloveError> {
        let status = foxglove::websocket::PlaybackStatus::try_from(self.status).map_err(|e| {
            foxglove::FoxgloveError::ValueError(format!("invalid playback status {e}"))
        })?;

        let request_id = if self.request_id.as_str().is_empty() {
            None
        } else {
            Some(self.request_id.into_string())
        };

        Ok(foxglove::websocket::PlaybackState {
            status,
            playback_speed: self.playback_speed,
            current_time: self.current_time,
            request_id,
        })
    }
}

impl Clone for FoxglovePlaybackState {
    fn clone(&self) -> Self {
        Self {
            status: self.status,
            current_time: self.current_time,
            playback_speed: self.playback_speed,
            request_id: self.request_id.clone(),
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_playback_state_create(
    playback_state: *mut *mut FoxglovePlaybackState,
) -> FoxgloveError {
    if playback_state.is_null() {
        return FoxgloveError::ValueError;
    }
    let this = Box::into_raw(Box::new(FoxglovePlaybackState {
        status: 0,
        current_time: 0,
        playback_speed: 0.0,
        request_id: FoxgloveStringBuf::new(String::new()),
    }));

    unsafe { *playback_state = this };
    FoxgloveError::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_playback_state_free(playback_state: *mut FoxglovePlaybackState) {
    if playback_state.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(playback_state) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_playback_state_set_request_id(
    playback_state: *mut FoxglovePlaybackState,
    request_id: FoxgloveString,
) -> FoxgloveError {
    if playback_state.is_null() {
        return FoxgloveError::ValueError;
    }

    let request_id = unsafe { request_id.as_utf8_str() };
    let Ok(request_id) = request_id else {
        return FoxgloveError::Utf8Error;
    };
    unsafe { (*playback_state).request_id = FoxgloveStringBuf::new(request_id.to_string()) };
    FoxgloveError::Ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_playback_state_clear_request_id(
    playback_state: *mut FoxglovePlaybackState,
) -> FoxgloveError {
    if playback_state.is_null() {
        return FoxgloveError::ValueError;
    }

    unsafe { (*playback_state).request_id = FoxgloveStringBuf::new(String::new()) };
    FoxgloveError::Ok
}
