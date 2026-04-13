use std::fmt;

use livekit::prelude::{DataTrackFrame, LocalDataTrack, PushFrameError};

/// Buffers data track frames while a LiveKit data track is being published.
///
/// Inserted synchronously into [`SessionState`] before the async `publish_data_track` call,
/// so that [`Sink::log`] can push frames immediately. Frames are buffered internally
/// until [`set_track`](Self::set_track) installs the published track, at which point the
/// buffer is drained. After that, frames are forwarded directly to the track.
pub(crate) struct DataTrackPublisher {
    inner: parking_lot::Mutex<DataTrackInner>,
}

struct DataTrackInner {
    track: Option<LocalDataTrack>,
    buffer: Vec<DataTrackFrame>,
}

/// Error returned by [`DataTrackPublisher::try_push`] when a frame cannot be delivered.
#[derive(Debug)]
pub(crate) enum TryPushError {
    /// The pre-publish buffer is full; the frame was dropped.
    BufferFull,
    /// The underlying data track rejected the frame.
    Track(PushFrameError),
}

impl fmt::Display for TryPushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferFull => write!(f, "pre-publish buffer full"),
            Self::Track(e) => write!(f, "{e}"),
        }
    }
}

impl From<PushFrameError> for TryPushError {
    fn from(e: PushFrameError) -> Self {
        Self::Track(e)
    }
}

impl DataTrackPublisher {
    /// Matches the LiveKit data track's internal `FRAME_BUFFER_COUNT`.
    pub(crate) const FRAME_BUFFER_CAPACITY: usize = 16;

    pub fn new() -> Self {
        Self {
            inner: parking_lot::Mutex::new(DataTrackInner {
                track: None,
                buffer: Vec::new(),
            }),
        }
    }

    /// Push a frame to the data track, buffering if the track is not yet published.
    ///
    /// When the track is available, any buffered frames are drained first.
    /// When the pre-publish buffer is full, new frames are dropped to preserve
    /// the earliest (potentially most important) frames.
    pub fn try_push(&self, frame: DataTrackFrame) -> Result<(), TryPushError> {
        let mut inner = self.inner.lock();
        let DataTrackInner { track, buffer } = &mut *inner;
        if let Some(track) = track {
            Self::drain_buffer(buffer, track);
            track.try_push(frame)?;
            Ok(())
        } else if buffer.len() < Self::FRAME_BUFFER_CAPACITY {
            buffer.push(frame);
            Ok(())
        } else {
            Err(TryPushError::BufferFull)
        }
    }

    /// Install the published track and drain any buffered frames into it.
    pub fn set_track(&self, track: LocalDataTrack) {
        let mut inner = self.inner.lock();
        Self::drain_buffer(&mut inner.buffer, &track);
        inner.track = Some(track);
    }

    /// Remove and return the track for unpublishing. Remaining buffered frames are dropped.
    pub fn take_track(&self) -> Option<LocalDataTrack> {
        let mut inner = self.inner.lock();
        inner.buffer.clear();
        inner.track.take()
    }

    fn drain_buffer(buffer: &mut Vec<DataTrackFrame>, track: &LocalDataTrack) {
        for frame in buffer.drain(..) {
            if track.try_push(frame).is_err() {
                break;
            }
        }
    }
}

impl fmt::Debug for DataTrackPublisher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.inner.lock();
        f.debug_struct("DataTrackPublisher")
            .field("has_track", &inner.track.is_some())
            .field("buffered_frames", &inner.buffer.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_before_track() {
        let publisher = DataTrackPublisher::new();
        for i in 0..10 {
            let frame = DataTrackFrame::new(vec![i]);
            assert!(publisher.try_push(frame).is_ok());
        }
        let inner = publisher.inner.lock();
        assert_eq!(inner.buffer.len(), 10);
    }

    #[test]
    fn test_buffer_full_drops_newest() {
        let publisher = DataTrackPublisher::new();
        for i in 0..DataTrackPublisher::FRAME_BUFFER_CAPACITY {
            let frame = DataTrackFrame::new(vec![i as u8]);
            assert!(publisher.try_push(frame).is_ok());
        }
        let overflow_frame = DataTrackFrame::new(vec![0xFF]);
        assert!(matches!(
            publisher.try_push(overflow_frame),
            Err(TryPushError::BufferFull)
        ));
        let inner = publisher.inner.lock();
        assert_eq!(
            inner.buffer.len(),
            DataTrackPublisher::FRAME_BUFFER_CAPACITY
        );
        assert_eq!(inner.buffer[0].payload()[0], 0);
        assert_eq!(
            inner.buffer.last().unwrap().payload()[0],
            (DataTrackPublisher::FRAME_BUFFER_CAPACITY - 1) as u8
        );
    }

    #[test]
    fn test_take_track_clears_buffer() {
        let publisher = DataTrackPublisher::new();
        for i in 0..5 {
            let frame = DataTrackFrame::new(vec![i]);
            let _ = publisher.try_push(frame);
        }
        assert!(publisher.take_track().is_none());
        let inner = publisher.inner.lock();
        assert!(inner.buffer.is_empty());
    }
}
