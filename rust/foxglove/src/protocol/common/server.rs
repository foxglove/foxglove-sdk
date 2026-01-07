//! Server messages.

pub mod advertise;
pub mod advertise_services;
pub mod connection_graph_update;
pub mod fetch_asset_response;
mod parameter_values;
#[doc(hidden)]
pub mod playback_state;
mod remove_status;
pub mod server_info;
mod service_call_failure;
mod service_call_response;
pub mod status;
mod time;
mod unadvertise;
mod unadvertise_services;

pub use advertise::{Advertise, Channel};
pub use advertise_services::AdvertiseServices;
pub use connection_graph_update::ConnectionGraphUpdate;
pub use fetch_asset_response::FetchAssetResponse;
pub use parameter_values::ParameterValues;
#[doc(hidden)]
pub use playback_state::PlaybackState;
pub use remove_status::RemoveStatus;
pub use server_info::ServerInfo;
pub use service_call_failure::ServiceCallFailure;
pub use service_call_response::ServiceCallResponse;
pub use status::Status;
pub use time::Time;
pub use unadvertise::Unadvertise;
pub use unadvertise_services::UnadvertiseServices;

#[repr(u8)]
pub(crate) enum BinaryOpcode {
    MessageData = 1,
    Time = 2,
    ServiceCallResponse = 3,
    FetchAssetResponse = 4,
    PlaybackState = 5,
}
impl BinaryOpcode {
    pub(crate) fn from_repr(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MessageData),
            2 => Some(Self::Time),
            3 => Some(Self::ServiceCallResponse),
            4 => Some(Self::FetchAssetResponse),
            5 => Some(Self::PlaybackState),
            _ => None,
        }
    }
}
