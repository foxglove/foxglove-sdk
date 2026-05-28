use std::{collections::HashMap, env, sync::OnceLock};

use log::LevelFilter;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_log::Logger;

/// Initialize pyo3 logging, ignoring errors if a logger has already been initialized.
pub(crate) fn init_logging(py: Python<'_>) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let Ok(env_var) = env::var("FOXGLOVE_LOG_LEVEL") else {
            let _ = pyo3_log::try_init();
            return;
        };

        let config = parse_log_env(&env_var);
        let mut logger = Logger::default();

        let mut global_level = None;
        for (target, level) in config {
            if target.is_empty() {
                logger = logger.filter(level);
                global_level = Some(level);
            } else {
                let is_foxglove_target = target == "foxglove";
                logger = logger.filter_target(target, level);
                if is_foxglove_target {
                    global_level = Some(level);
                }
            }
        }

        let _ = logger.install();
        // Configure Python logging module, if it hasn't been configured.
        // Without this FOXGLOVE_LOG_LEVEL won't take effect correctly,
        // Python would use the lastResort logger with Warn level.
        let _ = configure_python_logging(py, global_level.unwrap_or(LevelFilter::Warn).as_str());
    });
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
fn parse_log_env(spec: &str) -> HashMap<String, LevelFilter> {
    let mut directives = HashMap::new();

    // Discard the regex filter if present
    let mut parts = spec.split('/');
    let Some(spec) = parts.next() else {
        return HashMap::new();
    };

    for rule in spec.split(',') {
        let rule = rule.trim();
        if rule.is_empty() {
            continue;
        }

        if let Some((module, level_str)) = rule.split_once('=') {
            // target=level
            if let Some(level) = parse_level(level_str.trim()) {
                directives.insert(module.trim().to_string(), level);
            }
        } else if let Some(level) = parse_level(rule.trim()) {
            // level
            directives.insert("".to_string(), level);
        } else {
            // target
            directives.insert(rule.trim().to_string(), LevelFilter::Trace);
        }
    }

    directives
}

#[cfg(test)]
mod tests {
    use maplit::hashmap;

    use super::*;

    #[test]
    fn test_parse_log_env() {
        let config = parse_log_env("debug");
        assert_eq!(config, hashmap!("".to_string() => LevelFilter::Debug));

        let config = parse_log_env("debug,foxglove::websocket=info");
        assert_eq!(
            config,
            hashmap!(
              "".to_string() => LevelFilter::Debug,
              "foxglove::websocket".to_string() => LevelFilter::Info
            )
        );

        let config = parse_log_env("some_module");
        assert_eq!(
            config,
            hashmap!("some_module".to_string() => LevelFilter::Trace)
        );

        let config = parse_log_env("debug,some_module/foo");
        assert_eq!(
            config,
            hashmap!(
              "".to_string() => LevelFilter::Debug,
              "some_module".to_string() => LevelFilter::Trace
            )
        );

        let config = parse_log_env("");
        assert_eq!(config, HashMap::new());
    }
}
