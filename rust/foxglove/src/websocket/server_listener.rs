use super::{ChannelView, Client, ClientChannel, Parameter};

/// Provides a mechanism for registering callbacks for handling client message events.
///
/// These methods are invoked from the client's main poll loop and must not block. If blocking or
/// long-running behavior is required, the implementation should use [`tokio::task::spawn`] (or
/// [`tokio::task::spawn_blocking`]).
pub trait ServerListener: Send + Sync {
    /// Callback invoked when a client message is received.
    fn on_message_data(&self, _client: Client, _client_channel: &ClientChannel, _payload: &[u8]) {}
    /// Callback invoked when a client subscribes to a channel.
    /// Only invoked if the channel is associated with the server and isn't already subscribed to by the client.
    fn on_subscribe(&self, _client: Client, _channel: ChannelView) {}
    /// Callback invoked when a client unsubscribes from a channel or disconnects.
    /// Only invoked for channels that had an active subscription from the client.
    fn on_unsubscribe(&self, _client: Client, _channel: ChannelView) {}
    /// Callback invoked when a client advertises a client channel. Requires
    /// [`Capability::ClientPublish`][super::Capability::ClientPublish].
    fn on_client_advertise(&self, _client: Client, _channel: &ClientChannel) {}
    /// Callback invoked when a client unadvertises a client channel. Requires
    /// [`Capability::ClientPublish`][super::Capability::ClientPublish].
    fn on_client_unadvertise(&self, _client: Client, _channel: &ClientChannel) {}
    /// Callback invoked when a client requests parameters. Requires
    /// [`Capability::Parameters`][super::Capability::Parameters]. Should return the named
    /// paramters, or all paramters if param_names is empty.
    fn on_get_parameters(
        &self,
        _client: Client,
        _param_names: Vec<String>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        Vec::new()
    }
    /// Callback invoked when a client sets parameters. Requires
    /// [`Capability::Parameters`][super::Capability::Parameters].
    /// Should return the updated parameters for the passed parameters.
    /// The implementation could return the modified parameters.
    /// All clients subscribed to updates for the _returned_ parameters will be notified.
    ///
    /// Note that only `parameters` which have changed are included in the callback, but the return
    /// value must include all parameters.
    fn on_set_parameters(
        &self,
        _client: Client,
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
    /// Callback invoked when the first client subscribes to the connection graph. Requires
    /// [`Capability::ConnectionGraph`][super::Capability::ConnectionGraph].
    fn on_connection_graph_subscribe(&self) {}
    /// Callback invoked when the last client unsubscribes from the connection graph. Requires
    /// [`Capability::ConnectionGraph`][super::Capability::ConnectionGraph].
    fn on_connection_graph_unsubscribe(&self) {}
}
