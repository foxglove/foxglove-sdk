use crate::protocol::v2::server::server_info;

/// A capability that can be advertised by a [`RemoteAccessSink`](crate::RemoteAccessSink).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Allow clients to advertise channels to send data messages to the server.
    ClientPublish,
}

impl Capability {
    pub(crate) fn as_protocol_capabilities(&self) -> &'static [server_info::Capability] {
        match self {
            Self::ClientPublish => &[server_info::Capability::ClientPublish],
        }
    }
}
