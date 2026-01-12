//! Client messages.

pub mod advertise;
mod fetch_asset;
mod get_parameters;
mod playback_control_request;
mod service_call_request;
mod set_parameters;
mod subscribe_connection_graph;
mod subscribe_parameter_updates;
mod unadvertise;
mod unsubscribe_connection_graph;
mod unsubscribe_parameter_updates;

pub use advertise::Advertise;
pub use fetch_asset::FetchAsset;
pub use get_parameters::GetParameters;
#[doc(hidden)]
pub use playback_control_request::{PlaybackCommand, PlaybackControlRequest};
pub use service_call_request::ServiceCallRequest;
pub use set_parameters::SetParameters;
pub use subscribe_connection_graph::SubscribeConnectionGraph;
pub use subscribe_parameter_updates::SubscribeParameterUpdates;
pub use unadvertise::Unadvertise;
pub use unsubscribe_connection_graph::UnsubscribeConnectionGraph;
pub use unsubscribe_parameter_updates::UnsubscribeParameterUpdates;

#[repr(u8)]
pub(crate) enum BinaryOpcode {
    MessageData = 1,
    ServiceCallRequest = 2,
    #[doc(hidden)]
    PlaybackControlRequest = 3,
}
impl BinaryOpcode {
    pub(crate) fn from_repr(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MessageData),
            2 => Some(Self::ServiceCallRequest),
            3 => Some(Self::PlaybackControlRequest),
            _ => None,
        }
    }
}
