//! Provides an identifier for the library used as a log source.
use std::sync::{LazyLock, OnceLock};

static COMPILED_SDK_LANGUAGE: LazyLock<String> = LazyLock::new(|| {
    option_env!("FOXGLOVE_SDK_LANGUAGE")
        .unwrap_or("rust")
        .to_string()
});

static CELL: OnceLock<&'static str> = OnceLock::new();

/// Sets the language of the SDK. This should be called as soon as possible by an implementation,
/// otherwise the compiled language will be used when reporting the library version.
pub fn set_sdk_language(language: &'static str) {
    CELL.get_or_init(|| language);
}

/// Get the language of the SDK.
/// Note that `set_sdk_language` must be called before this for it to have an effect.
pub(crate) fn get_sdk_language() -> &'static str {
    CELL.get_or_init(|| COMPILED_SDK_LANGUAGE.as_str())
}

// Get the version of the SDK.
pub(crate) fn get_sdk_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Returns a user-agent-like SDK identifier for this library, for use in log sinks
/// and wire-visible metadata.
/// Note that `set_sdk_language` must be called before this for it to have an effect.
pub(crate) fn get_library_identifier() -> String {
    format!("foxglove-sdk-{}/{}", get_sdk_language(), get_sdk_version())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_library_identifier_uses_user_agent_like_token() {
        let library_identifier = get_library_identifier();
        let tokens = library_identifier.split(' ').collect::<Vec<_>>();

        assert_eq!(tokens.len(), 1);
        assert_eq!(
            library_identifier,
            concat!("foxglove-sdk-rust/", env!("CARGO_PKG_VERSION"))
        );
        assert!(!library_identifier.contains("/v"));
        assert!(!library_identifier.contains("mcap-rust/"));
    }
}
