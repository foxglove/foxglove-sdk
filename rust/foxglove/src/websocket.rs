//! Websocket functionality

mod advertise;
mod capability;
mod channel_view;
mod client;
mod client_channel;
mod connected_client;
mod connection_graph;
mod cow_vec;
mod fetch_asset;
pub(crate) mod handshake;
mod semaphore;
mod server;
mod server_listener;
pub mod service;
mod streams;
mod subscription;
#[cfg(test)]
mod tests;
#[doc(hidden)]
pub mod ws_protocol;

pub use capability::Capability;
pub use channel_view::ChannelView;
pub use client::{Client, ClientId};
pub use client_channel::{ClientChannel, ClientChannelId};
pub use connection_graph::ConnectionGraph;
pub use fetch_asset::{AssetHandler, AssetResponder};
pub(crate) use fetch_asset::{AsyncAssetHandlerFn, BlockingAssetHandlerFn};
pub use server::ShutdownHandle;
pub(crate) use server::{create_server, Server, ServerOptions};
pub use server_listener::ServerListener;
pub use streams::TlsIdentity;
pub use ws_protocol::parameter::{
    DecodeError as ParameterDecodeError, Parameter, ParameterType, ParameterValue,
};
pub use ws_protocol::server::status::{Level as StatusLevel, Status};
