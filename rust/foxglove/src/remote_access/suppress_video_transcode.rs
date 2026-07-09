//! The video-transcoding opt-out classifier for remote access.

use crate::channel::ChannelDescriptor;

/// Decides, per channel, whether to opt out of video transcoding over remote access.
///
/// This callback is invoked when a channel is registered. Returning `true` advertises the channel
/// without a video track, so its messages are delivered on the data plane unchanged. This is
/// required for compressed depth maps, whose pixel values encode depth and would be corrupted by
/// lossy video transcoding — [`is_compressed_depth_format`] classifies a compressed-depth `format`
/// string.
///
/// Configured via [`Gateway::suppress_video_transcode`] (this trait) or
/// [`Gateway::suppress_video_transcode_fn`] (a closure).
///
/// [`is_compressed_depth_format`]: crate::remote_access::is_compressed_depth_format
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
