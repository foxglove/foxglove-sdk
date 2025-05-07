use std::sync::Once;

#[repr(u8)]
pub enum FoxgloveLogLevel {
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
/// "warn", or "error".
///
/// Log styles (colors) may be configured with the FOXGLOVE_LOG_STYLE environment variable "never",
/// "always", or "auto" (default).
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_set_log_level(level: FoxgloveLogLevel) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let initial_level = match &level {
            FoxgloveLogLevel::Off => "off",
            FoxgloveLogLevel::Debug => "debug",
            FoxgloveLogLevel::Info => "info",
            FoxgloveLogLevel::Warn => "warn",
            FoxgloveLogLevel::Error => "error",
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
        foxglove_set_log_level(FoxgloveLogLevel::Info);
        foxglove_set_log_level(FoxgloveLogLevel::Debug);
    }
}
