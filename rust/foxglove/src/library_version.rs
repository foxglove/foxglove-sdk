//! Provides an identifier for the library used as a log source.
use std::sync::OnceLock;

use crate::SDK_LANGUAGE;

static CELL: OnceLock<&'static str> = OnceLock::new();

/// Sets the language of the SDK.
pub fn set_sdk_language(language: &'static str) {
    CELL.get_or_init(|| language);
}

/// Returns an identifer for this library, for use in log sinks.
pub(crate) fn get_library_version() -> String {
    let language = CELL.get_or_init(|| SDK_LANGUAGE.as_str());
    let version = env!("CARGO_PKG_VERSION");
    format!("foxglove-sdk-{}/v{}", language, version)
}
