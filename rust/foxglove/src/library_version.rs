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

/// Returns a user-agent-like identifier for this library, for use in log sinks
/// and wire-visible metadata.
/// Note that `set_sdk_language` must be called before this for it to have an effect.
pub(crate) fn get_library_version() -> String {
    format!(
        "foxglove-sdk-{}/{} {}",
        get_sdk_language(),
        get_sdk_version(),
        mcap::LIBRARY_IDENTIFIER
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_version_uses_user_agent_like_tokens() {
        let library_version = get_library_version();
        let tokens = library_version.split(' ').collect::<Vec<_>>();

        assert_eq!(tokens.len(), 2);
        assert_eq!(
            tokens[0],
            concat!("foxglove-sdk-rust/", env!("CARGO_PKG_VERSION"))
        );
        assert!(!tokens[0].contains("/v"));
        assert_eq!(tokens[1], mcap::LIBRARY_IDENTIFIER);
        assert!(tokens[1].starts_with("mcap-rust/"));
    }
}
