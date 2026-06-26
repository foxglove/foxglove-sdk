//! Provides an identifier for the library used as a log source.
use std::sync::{LazyLock, OnceLock, RwLock};

static COMPILED_SDK_LANGUAGE: LazyLock<String> = LazyLock::new(|| {
    option_env!("FOXGLOVE_SDK_LANGUAGE")
        .unwrap_or("rust")
        .to_string()
});

static CELL: OnceLock<&'static str> = OnceLock::new();
static LIBRARY_IDENTIFIER_PREFIX: LazyLock<RwLock<Option<String>>> =
    LazyLock::new(|| RwLock::new(None));

/// Sets the language of the SDK. This should be called as soon as possible by an implementation,
/// otherwise the compiled language will be used when reporting the library version.
pub fn set_sdk_language(language: &'static str) {
    CELL.get_or_init(|| language);
}

/// Sets a product token to prepend to this library's identifier.
///
/// This should be called as soon as possible by wrappers that identify a product built on top of
/// the SDK. Calling this function updates the token used for subsequently generated identifiers.
pub fn set_library_identifier_prefix(prefix: impl Into<String>) {
    let prefix = prefix.into();
    let prefix = (!prefix.is_empty()).then_some(prefix);
    *LIBRARY_IDENTIFIER_PREFIX
        .write()
        .expect("library identifier prefix lock poisoned") = prefix;
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
    let prefix = LIBRARY_IDENTIFIER_PREFIX
        .read()
        .expect("library identifier prefix lock poisoned");
    format_library_identifier(prefix.as_deref(), get_sdk_language(), get_sdk_version())
}

fn format_library_identifier(
    prefix: Option<&str>,
    sdk_language: &str,
    sdk_version: &str,
) -> String {
    let sdk_identifier = format!("foxglove-sdk-{sdk_language}/{sdk_version}");
    match prefix {
        Some(prefix) if !prefix.is_empty() => format!("{prefix} {sdk_identifier}"),
        _ => sdk_identifier,
    }
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

    #[test]
    fn library_identifier_prefix_is_prepended() {
        let library_identifier =
            format_library_identifier(Some("foxglove-bridge/1.2.3"), "cpp", "0.25.2");

        assert_eq!(
            library_identifier,
            "foxglove-bridge/1.2.3 foxglove-sdk-cpp/0.25.2"
        );
    }
}
