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

/// Returns an identifer for this library, for use in log sinks.
/// Note that `set_sdk_language` must be called before this for it to have an effect.
pub(crate) fn get_library_version() -> String {
    let language = CELL.get_or_init(|| COMPILED_SDK_LANGUAGE.as_str());
    let version = env!("CARGO_PKG_VERSION");
    format!("foxglove-sdk-{}/v{}", language, version)
}
