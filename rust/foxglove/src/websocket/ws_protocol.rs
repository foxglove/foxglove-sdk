//! Local re-exports of messages that now live in `crate::protocol` for backwards compatibility
pub mod client {
    pub use crate::protocol::v1::client::advertise;
    pub use crate::protocol::v1::client::subscribe;
    pub use crate::protocol::v1::client::{
        Advertise, ClientMessage, FetchAsset, GetParameters, MessageData, PlaybackCommand,
        PlaybackControlRequest, ServiceCallRequest, SetParameters, Subscribe,
        SubscribeConnectionGraph, SubscribeParameterUpdates, Subscription, Unadvertise,
        Unsubscribe, UnsubscribeConnectionGraph, UnsubscribeParameterUpdates,
    };
}

pub mod parameter {
    pub use crate::protocol::v1::parameter::*;
}

pub mod schema {
    pub use crate::protocol::v1::schema::*;
}

pub mod server {
    pub use crate::protocol::v1::server::advertise;
    pub use crate::protocol::v1::server::advertise_services;
    pub use crate::protocol::v1::server::connection_graph_update;
    pub use crate::protocol::v1::server::fetch_asset_response;
    pub use crate::protocol::v1::server::playback_state;
    pub use crate::protocol::v1::server::server_info;
    pub use crate::protocol::v1::server::status;
    pub use crate::protocol::v1::server::{
        Advertise, AdvertiseServices, Channel, ConnectionGraphUpdate, FetchAssetResponse,
        MessageData, ParameterValues, PlaybackState, RemoveStatus, ServerInfo, ServerMessage,
        ServiceCallFailure, ServiceCallResponse, Status, Time, Unadvertise, UnadvertiseServices,
    };
}

pub use crate::protocol::v1::tungstenite;

pub use crate::protocol::v1::{BinaryMessage, JsonMessage, ParseError};
