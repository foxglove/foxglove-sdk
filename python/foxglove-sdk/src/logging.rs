use std::{env, sync::Once};

use log::LevelFilter;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_log::Logger;

/// Initialize pyo3 logging, ignoring errors if a logger has already been initialized.
///
/// Only has an effect the first time it is called, otherwise returns false.
pub(crate) fn init_logging(py: Python<'_>, level: Option<LevelFilter>) -> bool {
    static INIT: Once = Once::new();
    let mut initialized = false;
    INIT.call_once(|| {
        initialized = true;
        // Prefer FOXGLOVE_LOG_LEVEL over level, as we do in C++
        // The person running the program knows what logging they want better than the person writing the program
        let config = match env::var("FOXGLOVE_LOG_LEVEL") {
            Ok(val) => parse_log_env(&val),
            Err(_) => {
                if let Some(level) = level {
                    vec![("foxglove".to_string(), level)]
                } else {
                    // If no log level was passed and FOXGLOVE_LOG_LEVEL was not set, go with the default pyo3_log setup
                    let _ = pyo3_log::try_init();
                    return;
                }
            }
        };

        let mut logger = Logger::default().filter(LevelFilter::Warn);

        let mut global_level = None;
        for (target, level) in config {
            if target.is_empty() {
                global_level = Some(level);
                logger = logger.filter(level);
            } else {
                if target == "foxglove" {
                    global_level = Some(level);
                }
                logger = logger.filter_target(target, level);
            }
        }

        let _ = logger.install();
        // Configure Python logging module, if it hasn't been configured.
        // Without this FOXGLOVE_LOG_LEVEL won't take effect correctly,
        // Python would use the lastResort logger with Warn level.
        let _ = configure_python_logging(
            py,
            python_logging_level(global_level.unwrap_or(LevelFilter::Warn)),
        );
    });
    initialized
}

fn python_logging_level(level: LevelFilter) -> &'static str {
    match level {
        LevelFilter::Off => "CRITICAL",
        LevelFilter::Error => "ERROR",
        LevelFilter::Warn => "WARNING",
        LevelFilter::Info => "INFO",
        LevelFilter::Debug | LevelFilter::Trace => "DEBUG",
    }
}

fn configure_python_logging(py: Python<'_>, level: &str) -> PyResult<()> {
    let logging = py.import("logging")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("level", level)?;
    kwargs.set_item("format", "%(asctime)s [%(levelname)s] %(message)s")?;
    logging.call_method("basicConfig", (), Some(&kwargs))?;
    Ok(())
}

/// Parse a level string, corresponding to values of env_logger's RUST_LOG
fn parse_level(s: &str) -> Option<LevelFilter> {
    match s.to_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "info" => Some(LevelFilter::Info),
        "warn" => Some(LevelFilter::Warn),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

/// Parse a subset of the patterns supported by env_logger's RUST_LOG environment variable.
///
/// The variable consists of one or more comma-separated directives:
///
/// ```text
///   RUST_LOG=[target][=][level][,...]
/// ```
///
/// Regex filters (a trailing slash + pattern) are ignored.
fn parse_log_env(spec: &str) -> Vec<(String, LevelFilter)> {
    let mut directives = Vec::new();

    // Discard the regex filter if present
    let mut parts = spec.split('/');
    let Some(spec) = parts.next() else {
        return Vec::new();
    };

    for rule in spec.split(',') {
        let rule = rule.trim();
        if rule.is_empty() {
            continue;
        }

        if let Some((module, level_str)) = rule.split_once('=') {
            // target=level
            if let Some(level) = parse_level(level_str.trim()) {
                directives.push((module.trim().to_string(), level));
            }
        } else if let Some(level) = parse_level(rule.trim()) {
            // level
            directives.push(("".to_string(), level));
        } else {
            // target
            directives.push((rule.trim().to_string(), LevelFilter::Trace));
        }
    }

    directives
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_env() {
        let config = parse_log_env("debug");
        assert_eq!(config, vec![("".to_string(), LevelFilter::Debug)]);

        let config = parse_log_env("debug,foxglove::websocket=info");
        assert_eq!(
            config,
            vec![
                ("".to_string(), LevelFilter::Debug),
                ("foxglove::websocket".to_string(), LevelFilter::Info),
            ]
        );

        let config = parse_log_env("some_module");
        assert_eq!(
            config,
            vec![("some_module".to_string(), LevelFilter::Trace)]
        );

        let config = parse_log_env("debug,some_module/foo");
        assert_eq!(
            config,
            vec![
                ("".to_string(), LevelFilter::Debug),
                ("some_module".to_string(), LevelFilter::Trace),
            ]
        );

        let config = parse_log_env("");
        assert_eq!(config, Vec::new());
    }

    #[test]
    fn test_python_logging_level() {
        assert_eq!(python_logging_level(LevelFilter::Off), "CRITICAL");
        assert_eq!(python_logging_level(LevelFilter::Error), "ERROR");
        assert_eq!(python_logging_level(LevelFilter::Warn), "WARNING");
        assert_eq!(python_logging_level(LevelFilter::Info), "INFO");
        assert_eq!(python_logging_level(LevelFilter::Debug), "DEBUG");
        assert_eq!(python_logging_level(LevelFilter::Trace), "DEBUG");
    }
}
