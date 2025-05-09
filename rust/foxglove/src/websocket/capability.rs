use super::ws_protocol::server::server_info;

/// A capability that a websocket server can support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
    /// Allow clients to get & set parameters, and subscribe to updates.
    Parameters,
    /// Inform clients about the latest server time.
    ///
    /// This allows accelerated, slowed, or stepped control over the progress of time. If the
    /// server publishes time data, then timestamps of published messages must originate from the
    /// same time source.
    Time,
    /// Allow clients to call services.
    Services,
    /// Allow clients to request assets. If you supply an asset handler to the server, this
    /// capability will be advertised automatically.
    Assets,
    /// Allow clients to subscribe and make connection graph updates
    ConnectionGraph,
}

impl Capability {
    pub(crate) fn as_protocol_capabilities(&self) -> &'static [server_info::Capability] {
        match self {
            Self::ClientPublish => &[server_info::Capability::ClientPublish],
            Self::Parameters => &[
                server_info::Capability::Parameters,
                server_info::Capability::ParametersSubscribe,
            ],
            Self::Time => &[server_info::Capability::Time],
            Self::Services => &[server_info::Capability::Services],
            Self::Assets => &[server_info::Capability::Assets],
            Self::ConnectionGraph => &[server_info::Capability::ConnectionGraph],
        }
    }
}
