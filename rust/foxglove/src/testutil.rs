//! Test utilities.

mod global_context;
mod mcap;
mod sink;
#[cfg(feature = "websocket")]
mod websocket;
#[cfg(feature = "websocket")]
mod websocket_client;

pub use global_context::GlobalContextTest;
pub(crate) use mcap::read_summary;
pub use sink::{ErrorSink, MockSink, RecordingSink};
#[cfg(feature = "websocket")]
pub(crate) use websocket::{RecordingServerListener, assert_eventually};
#[cfg(feature = "websocket")]
pub use websocket_client::{WebSocketClient, WebSocketClientError};
