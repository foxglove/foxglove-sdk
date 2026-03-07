//! Shared helpers for parsing netem argument strings used by netem test suites.

/// Parse the base delay (in ms) from a netem args string.
///
/// Matches the first `delay <N>ms` token pair. Returns `None` if no delay is
/// configured or the value cannot be parsed.
///
/// ```text
/// "delay 200ms 50ms loss 5%" → Some(200)
/// "loss 5%"                  → None
/// ```
pub fn parse_delay_ms(netem_args: &str) -> Option<u64> {
    netem_args
        .split_whitespace()
        .zip(netem_args.split_whitespace().skip(1))
        .find(|(key, _)| *key == "delay")
        .and_then(|(_, val)| val.strip_suffix("ms")?.parse().ok())
}

/// Parse the loss percentage from a netem args string.
///
/// Matches the first `loss <N>%` token pair. Returns `None` if no loss is
/// configured or the value cannot be parsed.
///
/// ```text
/// "delay 200ms 50ms loss 5%" → Some(5.0)
/// "delay 10ms 2ms"           → None
/// ```
pub fn parse_loss_percentage(netem_args: &str) -> Option<f64> {
    netem_args
        .split_whitespace()
        .zip(netem_args.split_whitespace().skip(1))
        .find(|(key, _)| *key == "loss")
        .and_then(|(_, val)| val.strip_suffix('%')?.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delay_basic() {
        assert_eq!(parse_delay_ms("delay 200ms 50ms loss 5%"), Some(200));
        assert_eq!(parse_delay_ms("delay 10ms 2ms"), Some(10));
        assert_eq!(parse_delay_ms("loss 5%"), None);
        assert_eq!(parse_delay_ms(""), None);
    }

    #[test]
    fn parse_loss_basic() {
        assert_eq!(parse_loss_percentage("delay 200ms 50ms loss 5%"), Some(5.0));
        assert_eq!(parse_loss_percentage("delay 10ms 2ms"), None);
        assert_eq!(parse_loss_percentage("loss 0.1%"), Some(0.1));
        assert_eq!(parse_loss_percentage(""), None);
    }
}
