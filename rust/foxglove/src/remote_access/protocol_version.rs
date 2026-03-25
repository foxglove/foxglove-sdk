//! Remote access protocol version constants.

/// The remote access protocol version supported by this SDK build.
pub(crate) const REMOTE_ACCESS_PROTOCOL_VERSION: &str = "2.0.1";

/// The minimum remote access protocol version this SDK will accept from a connecting participant.
pub(crate) const REMOTE_ACCESS_MIN_SUPPORTED_PROTOCOL_VERSION: &str = "2.0.0";

/// The protocol version assumed when a participant does not advertise one.
///
/// This is the version that was in use before version advertisement was introduced.
pub(crate) const DEFAULT_PROTOCOL_VERSION: &str = "2.0.0";
