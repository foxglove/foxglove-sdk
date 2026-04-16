//! Remote access protocol version constants and helpers.

use std::collections::HashMap;

use livekit::id::ParticipantIdentity;
use tracing::error;

/// The LiveKit participant attribute key used to advertise the remote access protocol version.
pub(crate) const PROTOCOL_VERSION_ATTRIBUTE: &str = "protocolVersion";

/// The remote access protocol version supported by this SDK build.
pub(crate) const REMOTE_ACCESS_PROTOCOL_VERSION: semver::Version = semver::Version::new(2, 2, 0);

/// The minimum remote access protocol version this SDK will accept from a connecting participant.
pub(crate) const REMOTE_ACCESS_MIN_SUPPORTED_PROTOCOL_VERSION: semver::Version =
    semver::Version::new(2, 2, 0);

/// The protocol version assumed when a participant does not advertise one.
pub(crate) const DEFAULT_PROTOCOL_VERSION: semver::Version = semver::Version::new(2, 2, 0);

/// Parse the remote access protocol version from a LiveKit participant's attributes.
///
/// If the attribute is absent the participant is assumed to be running a pre-advertisement
/// build, so we default to [`DEFAULT_PROTOCOL_VERSION`].
///
/// Returns `None` if the attribute value is present but cannot be parsed as a semver triple.
pub(crate) fn parse_participant_protocol_version(
    attributes: &HashMap<String, String>,
) -> Option<semver::Version> {
    let Some(version_str) = attributes.get(PROTOCOL_VERSION_ATTRIBUTE) else {
        return Some(DEFAULT_PROTOCOL_VERSION.clone());
    };
    match semver::Version::parse(version_str) {
        Ok(v) => Some(v),
        Err(e) => {
            error!(
                version = version_str.as_str(),
                "failed to parse participant protocol version: {e}"
            );
            None
        }
    }
}

/// Check whether a participant's protocol version is compatible.
///
/// A version is compatible if it is at or above the minimum supported version and its major
/// version matches ours. A higher major version indicates breaking changes that this build does
/// not understand, so it is rejected just as an older-than-minimum version would be.
///
/// Returns the parsed version if compatible, or `None` if the participant should be rejected.
/// Callers should use [`REMOTE_ACCESS_PROTOCOL_VERSION`] (not the minimum) when reporting an
/// incompatibility to the user, since the minimum does not cover major-version mismatches.
pub(crate) fn check_participant_protocol_version(
    participant_identity: &ParticipantIdentity,
    attributes: &HashMap<String, String>,
    remote_access_session_id: Option<&str>,
) -> Option<semver::Version> {
    let version = parse_participant_protocol_version(attributes)?;
    if version < REMOTE_ACCESS_MIN_SUPPORTED_PROTOCOL_VERSION {
        error!(
            remote_access_session_id,
            participant_identity = %participant_identity,
            participant_version = %version,
            min_supported_version = %REMOTE_ACCESS_MIN_SUPPORTED_PROTOCOL_VERSION,
            "participant protocol version is below minimum supported; ignoring participant"
        );
        return None;
    }
    if version.major != REMOTE_ACCESS_PROTOCOL_VERSION.major {
        error!(
            remote_access_session_id,
            participant_identity = %participant_identity,
            participant_version = %version,
            our_version = %REMOTE_ACCESS_PROTOCOL_VERSION,
            "participant protocol version has incompatible major version; ignoring participant"
        );
        return None;
    }
    Some(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(version: &str) -> HashMap<String, String> {
        HashMap::from([(PROTOCOL_VERSION_ATTRIBUTE.to_string(), version.to_string())])
    }

    fn identity() -> ParticipantIdentity {
        ParticipantIdentity::from("test-participant".to_string())
    }

    // --- parse_participant_protocol_version ---

    #[test]
    fn parse_valid_version() {
        let result = parse_participant_protocol_version(&attrs("2.2.0"));
        assert_eq!(result, Some(semver::Version::new(2, 2, 0)));
    }

    #[test]
    fn parse_missing_attribute_defaults_to_current() {
        let result = parse_participant_protocol_version(&HashMap::new());
        assert_eq!(result, Some(semver::Version::new(2, 2, 0)));
    }

    #[test]
    fn parse_garbage_string_returns_none() {
        let result = parse_participant_protocol_version(&attrs("not-a-version"));
        assert_eq!(result, None);
    }

    #[test]
    fn parse_empty_string_returns_none() {
        let result = parse_participant_protocol_version(&attrs(""));
        assert_eq!(result, None);
    }

    // --- check_participant_protocol_version ---

    #[test]
    fn check_valid_version_at_minimum_returns_some() {
        let result = check_participant_protocol_version(&identity(), &attrs("2.2.0"), None);
        assert_eq!(result, Some(semver::Version::new(2, 2, 0)));
    }

    #[test]
    fn check_valid_version_above_minimum_returns_some() {
        let result = check_participant_protocol_version(&identity(), &attrs("2.2.1"), Some("sess"));
        assert_eq!(result, Some(semver::Version::new(2, 2, 1)));
    }

    #[test]
    fn check_missing_attribute_defaults_and_passes() {
        // DEFAULT_PROTOCOL_VERSION == REMOTE_ACCESS_MIN_SUPPORTED_PROTOCOL_VERSION == 2.2.0
        let result = check_participant_protocol_version(&identity(), &HashMap::new(), None);
        assert_eq!(result, Some(semver::Version::new(2, 2, 0)));
    }

    #[test]
    fn check_version_below_minimum_returns_none() {
        let result = check_participant_protocol_version(&identity(), &attrs("2.1.9"), None);
        assert_eq!(result, None);
    }

    #[test]
    fn check_future_major_version_returns_none() {
        let result = check_participant_protocol_version(&identity(), &attrs("3.0.0"), None);
        assert_eq!(result, None);
    }

    #[test]
    fn check_garbage_string_returns_none() {
        let result = check_participant_protocol_version(&identity(), &attrs("garbage"), None);
        assert_eq!(result, None);
    }
}
