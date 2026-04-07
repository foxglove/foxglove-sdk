use crate::protocol::v2::server::server_info;

/// A capability that can be advertised by a [`Gateway`](super::Gateway).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
    /// Allow clients to get, set, and subscribe to parameter updates.
    Parameters,
    /// Allow clients to call services.
    Services,
    /// Allow clients to request assets. If you supply an asset handler to the gateway, this
    /// capability will be advertised automatically.
    Assets,
    /// Allow clients to subscribe and make connection graph updates.
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
            Self::Services => &[server_info::Capability::Services],
            Self::Assets => &[server_info::Capability::Assets],
            Self::ConnectionGraph => &[server_info::Capability::ConnectionGraph],
        }
    }
}
