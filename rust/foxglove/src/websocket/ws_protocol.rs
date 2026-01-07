//! Local re-exports of messages that now live in `crate::protocol` for backwards compatibility
pub mod client {
    pub use crate::protocol::common::client::advertise;
    pub use crate::protocol::v1::client::subscribe;
    pub use crate::protocol::v1::client::{
        Advertise, ClientMessageV1 as ClientMessage, FetchAsset, GetParameters,
        MessageDataV1 as MessageData, PlaybackCommand, PlaybackControlRequest, ServiceCallRequest,
        SetParameters, SubscribeConnectionGraph, SubscribeParameterUpdates,
        SubscribeV1 as Subscribe, Subscription, Unadvertise, UnsubscribeConnectionGraph,
        UnsubscribeParameterUpdates, UnsubscribeV1 as Unsubscribe,
    };
}

pub mod parameter {
    pub use crate::protocol::common::parameter::*;
}

pub mod schema {
    pub use crate::protocol::common::schema::*;
}

pub mod server {
    pub use crate::protocol::common::server::advertise;
    pub use crate::protocol::common::server::advertise_services;
    pub use crate::protocol::common::server::connection_graph_update;
    pub use crate::protocol::common::server::fetch_asset_response;
    pub use crate::protocol::common::server::playback_state;
    pub use crate::protocol::common::server::server_info;
    pub use crate::protocol::common::server::status;
    pub use crate::protocol::v1::server::{
        Advertise, AdvertiseServices, ConnectionGraphUpdate, FetchAssetResponse,
        MessageDataV1 as MessageData, ParameterValues, PlaybackState, RemoveStatus, ServerInfo,
        ServerMessageV1 as ServerMessage, ServiceCallFailure, ServiceCallResponse, Status, Time,
        Unadvertise, UnadvertiseServices,
    };
}

pub use crate::protocol::common::tungstenite;

pub use crate::protocol::common::{BinaryMessage, JsonMessage, ParseError};
