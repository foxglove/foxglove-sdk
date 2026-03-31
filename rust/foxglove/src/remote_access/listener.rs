use crate::ChannelDescriptor;
use crate::protocol::v2::parameter::Parameter;

use super::client::Client;
use super::connection::ConnectionStatus;

/// Provides a mechanism for registering callbacks for handling client message events.
///
/// These methods are invoked from time-sensitive contexts and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
pub trait Listener: Send + Sync {
    /// Callback invoked when the gateway connection status changes.
    fn on_connection_status_changed(&self, _status: ConnectionStatus) {}
    /// Callback invoked when a client message is received.
    fn on_message_data(&self, _client: &Client, _channel: &ChannelDescriptor, _payload: &[u8]) {}
    /// Callback invoked when a client subscribes to a channel.
    fn on_subscribe(&self, _client: &Client, _channel: &ChannelDescriptor) {}
    /// Callback invoked when a client unsubscribes from a channel or disconnects.
    fn on_unsubscribe(&self, _client: &Client, _channel: &ChannelDescriptor) {}
    /// Callback invoked when a client advertises a client channel.
    fn on_client_advertise(&self, _client: &Client, _channel: &ChannelDescriptor) {}
    /// Callback invoked when a client unadvertises a client channel.
    fn on_client_unadvertise(&self, _client: &Client, _channel: &ChannelDescriptor) {}
    /// Callback invoked when a client requests parameters. Requires
    /// [`Capability::Parameters`][super::Capability::Parameters]. Should return the named
    /// parameters, or all parameters if `param_names` is empty.
    fn on_get_parameters(
        &self,
        _client: &Client,
        _param_names: Vec<String>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        Vec::new()
    }
    /// Callback invoked when a client sets parameters. Requires
    /// [`Capability::Parameters`][super::Capability::Parameters].
    /// Should return the updated parameters for the passed parameters.
    /// All clients subscribed to updates for the _returned_ parameters will be notified.
    /// If this callback returns parameters that are unset (i.e. have a `None` value),
    /// the unset parameters will not be published to clients.
    ///
    /// Note that only `parameters` which have changed are included in the callback, but the return
    /// value must include all parameters.
    fn on_set_parameters(
        &self,
        _client: &Client,
        parameters: Vec<Parameter>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        parameters
    }
    /// Callback invoked when a client subscribes to the named parameters for the first time.
    /// Requires [`Capability::Parameters`][super::Capability::Parameters].
    fn on_parameters_subscribe(&self, _param_names: Vec<String>) {}
    /// Callback invoked when the last client unsubscribes from the named parameters.
    /// Requires [`Capability::Parameters`][super::Capability::Parameters].
    fn on_parameters_unsubscribe(&self, _param_names: Vec<String>) {}
}
