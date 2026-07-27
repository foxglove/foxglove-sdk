//! The video-transcoding opt-out predicate for remote access.

use crate::channel::ChannelDescriptor;

/// Decides, per channel, whether to opt out of video transcoding over remote access.
///
/// This callback is invoked when a channel is registered. Returning `true` advertises the channel
/// without a video track, so its messages are delivered on the data plane unchanged. Use it for
/// image channels whose pixel values must not pass through lossy video — compressed depth maps are
/// the motivating case, since their pixels encode depth. The SDK cannot tell such channels apart
/// from a channel descriptor alone, so the producer identifies them here (e.g. by topic or schema).
///
/// Configured via [`Gateway::suppress_video_transcode`] (this trait) or
/// [`Gateway::suppress_video_transcode_fn`] (a closure).
///
/// [`Gateway::suppress_video_transcode`]: crate::remote_access::Gateway::suppress_video_transcode
/// [`Gateway::suppress_video_transcode_fn`]: crate::remote_access::Gateway::suppress_video_transcode_fn
pub trait SuppressVideoTranscode: Sync + Send {
    /// Returns `true` if the channel should be delivered as data rather than transcoded to video.
    fn should_suppress(&self, channel: &ChannelDescriptor) -> bool;
}

pub(super) struct SuppressVideoTranscodeFn<F>(pub(super) F)
where
    F: Fn(&ChannelDescriptor) -> bool + Sync + Send;

impl<F> SuppressVideoTranscode for SuppressVideoTranscodeFn<F>
where
    F: Fn(&ChannelDescriptor) -> bool + Sync + Send,
{
    fn should_suppress(&self, channel: &ChannelDescriptor) -> bool {
        self.0(channel)
    }
}
