//! WebSocket functionality

mod advertise;
mod capability;
mod channel_view;
mod client;
mod client_channel;
mod connected_client;
mod cow_vec;
pub(crate) mod handshake;
mod server;
mod server_listener;
pub mod service;
mod streams;
mod subscription;
#[cfg(test)]
mod tests;
#[doc(hidden)]
pub mod ws_protocol;

pub(crate) use crate::remote_common::fetch_asset::{AsyncAssetHandlerFn, BlockingAssetHandlerFn};
pub use crate::remote_common::{
    AnyClient, AssetHandler, AssetResponder, ClientId, ConnectionGraph, GetParametersResponder,
    Parameter, ParameterDecodeError, ParameterHandler, ParameterType, ParameterValue,
    SetParametersResponder, Status, StatusLevel,
};
pub use capability::Capability;
pub use channel_view::ChannelView;
pub use client::Client;
pub use client_channel::{ClientChannel, ClientChannelId};
pub use server::ShutdownHandle;
pub(crate) use server::{Server, ServerOptions, create_server};
pub use server_listener::ServerListener;
pub use streams::TlsIdentity;
pub use ws_protocol::client::{PlaybackCommand, PlaybackControlRequest};
pub use ws_protocol::server::playback_state::{PlaybackState, PlaybackStatus};
