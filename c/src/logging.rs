use std::sync::Once;

/// Logging level for the Foxglove SDK.
///
/// Used with `foxglove_set_log_level`.
#[repr(u8)]
pub enum FoxgloveLoggingLevel {
    Off = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

/// Initialize SDK logging with the given severity level.
///
/// The SDK logs informational messages to stderr. Any messages below the given level are not
/// logged.
///
/// This function should be called before other Foxglove initialization to capture output from all
/// components. Subsequent calls will have no effect.
///
/// Log level may be overridden with the FOXGLOVE_LOG_LEVEL environment variable: "debug", "info",
/// "warn", "error", or "off". The default level is "info".
///
/// Log styles (colors) may be configured with the FOXGLOVE_LOG_STYLE environment variable "never",
/// "always", or "auto" (default).
///
/// This is thread-safe, but only the first call to this function will have an effect.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_set_log_level(level: FoxgloveLoggingLevel) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let initial_level = match &level {
            FoxgloveLoggingLevel::Off => "off",
            FoxgloveLoggingLevel::Debug => "debug",
            FoxgloveLoggingLevel::Info => "info",
            FoxgloveLoggingLevel::Warn => "warn",
            FoxgloveLoggingLevel::Error => "error",
        };

        let env = env_logger::Env::default()
            .filter_or("FOXGLOVE_LOG_LEVEL", initial_level)
            .write_style_or("FOXGLOVE_LOG_STYLE", "auto");

        env_logger::Builder::from_env(env)
            .target(env_logger::Target::Stderr)
            .init();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foxglove_set_log_level_called_twice() {
        // env_logger panics if initialized twice; ensure we don't
        foxglove_set_log_level(FoxgloveLoggingLevel::Info);
        foxglove_set_log_level(FoxgloveLoggingLevel::Debug);
    }
}
