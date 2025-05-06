use std::sync::Once;

use log::LevelFilter;

#[repr(u8)]
pub enum FoxgloveLogSeverityLevel {
    Off = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

#[unsafe(no_mangle)]
pub extern "C" fn foxglove_set_log_severity_level(level: FoxgloveLogSeverityLevel) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let initial_level = match &level {
            FoxgloveLogSeverityLevel::Off => "off",
            FoxgloveLogSeverityLevel::Debug => "debug",
            FoxgloveLogSeverityLevel::Info => "info",
            FoxgloveLogSeverityLevel::Warn => "warn",
            FoxgloveLogSeverityLevel::Error => "error",
        };
        let env = env_logger::Env::default().default_filter_or(initial_level);
        env_logger::init_from_env(env);
    });

    log::set_max_level(match level {
        FoxgloveLogSeverityLevel::Off => LevelFilter::Off,
        FoxgloveLogSeverityLevel::Debug => LevelFilter::Debug,
        FoxgloveLogSeverityLevel::Info => LevelFilter::Info,
        FoxgloveLogSeverityLevel::Warn => LevelFilter::Warn,
        FoxgloveLogSeverityLevel::Error => LevelFilter::Error,
    });
}
