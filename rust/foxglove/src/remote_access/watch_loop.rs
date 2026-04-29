//! Pure decision logic for the dormant-phase watch loop.
//!
//! The shell in `connection.rs` is responsible for I/O (opening the watch stream, sleeping,
//! flipping connection status, cancelling the gateway). This module owns the *policy*: given a
//! connect error or a terminal [`WatchOutcome`], how should we mutate the retry state and what
//! should the shell do next? Keeping the policy pure lets us cover the branchy cases with cheap
//! synchronous tests.

use std::time::Duration;

use crate::api_client::WatchWakeEvent;

use super::watch::{HeartbeatExit, WatchError, WatchOutcome};

/// Backoff applied after transient watch-loop failures, capped at this value. Starts small and
/// doubles up to the cap. Reset on a successful connect.
pub(super) const MAX_BACKOFF: Duration = Duration::from_secs(30);
pub(super) const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// Backoff applied when another gateway holds the lease (409 Conflict). Picked conservatively
/// since the API owns lease TTLs and does not advertise them to the gateway.
pub(super) const LEASE_CONFLICT_BACKOFF: Duration = Duration::from_secs(30);

/// Mutable state carried across iterations of the watch loop.
pub(super) struct WatchRetryState {
    /// Lease ID of the previous watch, threaded into the next connect attempt so the API can
    /// short-circuit a conflict against our own prior lease. Cleared once a fresh watch is
    /// established or once the API tells us another lease owns the device.
    previous_lease_id: Option<String>,
    /// Backoff applied to the next transient retry. Doubled per failure up to [`MAX_BACKOFF`];
    /// reset by [`on_connect_success`].
    backoff: Duration,
}

impl WatchRetryState {
    pub fn new() -> Self {
        Self {
            previous_lease_id: None,
            backoff: INITIAL_BACKOFF,
        }
    }

    fn double_backoff(&mut self) {
        self.backoff = self.backoff.saturating_mul(2).min(MAX_BACKOFF);
    }

    pub fn previous_lease_id(&self) -> Option<&str> {
        self.previous_lease_id.as_deref()
    }
}

/// What the shell should do after a failed [`super::watch::Watch::connect`] attempt.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ConnectAction {
    /// Transition to `Connecting`, sleep for `delay`, then retry the connect.
    RetryAfter(Duration),
    /// Cancel the gateway and stop the watch loop.
    StopUnauthorized,
}

/// What the shell should do after a [`WatchOutcome`] terminates a connected watch.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WatchAction {
    /// A `wake` arrived: stop the watch loop and proceed to LiveKit.
    Wake(WatchWakeEvent),
    /// Soft reconnect: iterate immediately. The shell does NOT flip status to `Connecting`;
    /// the next connect attempt will only flip if it fails, and `connect_watch` owns the
    /// backoff schedule from there. Used for blips we expect to recover from quickly
    /// (LB-induced transport drops, read-timeouts, clean stream-end, lease gone). State
    /// updates (previous lease ID) have already been applied to the [`WatchRetryState`].
    Reconnect,
    /// Hard backoff: a known long disruption. The shell flips status to `Connecting` before
    /// sleeping `delay` and iterating. Used when we know we'll be off the wire long enough
    /// that callers should see the degradation (e.g. lease conflict).
    Backoff { delay: Duration },
    /// Cancel the gateway and stop the watch loop (e.g. heartbeat returned 401).
    StopUnauthorized,
    /// Stop the watch loop without cancelling. Reserved for the defensive "heartbeat task
    /// dropped its sender" path, which only fires if the heartbeat task panicked or was
    /// externally aborted before reporting a terminal reason.
    Stop,
}

/// Apply the state mutations that follow a successful [`super::watch::Watch::connect`]: drop
/// the previous lease ID (it has now been replaced) and reset the transient-retry backoff.
pub(super) fn on_connect_success(retry: &mut WatchRetryState) {
    retry.previous_lease_id = None;
    retry.backoff = INITIAL_BACKOFF;
}

/// Classify a connect error and update `retry` accordingly.
pub(super) fn on_connect_error(err: &WatchError, retry: &mut WatchRetryState) -> ConnectAction {
    if matches!(err, WatchError::Unauthorized) {
        return ConnectAction::StopUnauthorized;
    }
    let delay = match err {
        WatchError::Conflict => {
            // Another gateway owns the device. Our previous lease is irrelevant; drop it so
            // the next attempt does not advertise it.
            retry.previous_lease_id = None;
            LEASE_CONFLICT_BACKOFF
        }
        _ => {
            let delay = retry.backoff;
            retry.double_backoff();
            delay
        }
    };
    ConnectAction::RetryAfter(delay)
}

/// Classify the terminal outcome of a connected watch and update `retry` accordingly.
///
/// `lease_id` is captured from the watch's `hello` before it was closed, and is threaded into
/// the next connect for transient-error reconnects.
///
/// `watch_duration` is how long the watch was running. `heartbeat_interval` is the value
/// advertised by the server in the `hello` event. A `StreamError` after the watch ran for at
/// least `heartbeat_interval` is treated as a soft blip (LB-style transient drop) and retries
/// immediately. Earlier errors fall through to backoff to avoid hot-looping.
pub(super) fn on_outcome(
    outcome: WatchOutcome,
    lease_id: String,
    watch_duration: Duration,
    heartbeat_interval: Duration,
    retry: &mut WatchRetryState,
) -> WatchAction {
    match outcome {
        WatchOutcome::Wake(wake) => WatchAction::Wake(wake),
        // Read-timeout and clean stream-end are normal protocol behaviour. Tthe API closed the
        // dormant stream. Retry immediately and reuse our lease.
        WatchOutcome::ReadTimeout | WatchOutcome::StreamEnded => {
            retry.previous_lease_id = Some(lease_id);
            WatchAction::Reconnect
        }
        // Transport errors split on whether the watch ran long enough to be considered
        // healthy. A long-lived watch that errors is almost certainly an LB-driven drop:
        // retry immediately and stay Connected. A short-lived watch that errors signals a
        // real fault:  apply backoff and surface Connecting to callers.
        WatchOutcome::StreamError(_) => {
            retry.previous_lease_id = Some(lease_id);
            if watch_duration >= heartbeat_interval {
                WatchAction::Reconnect
            } else {
                let delay = retry.backoff;
                retry.double_backoff();
                WatchAction::Backoff { delay }
            }
        }
        WatchOutcome::HeartbeatLost(reason) => match reason {
            // Another gateway took over: drop our lease ID so the next connect does not
            // advertise it, and back off conservatively. The 30s wait is long enough that
            // callers should see the degradation, so we ask the shell to flip to Connecting.
            HeartbeatExit::Conflict => {
                retry.previous_lease_id = None;
                WatchAction::Backoff {
                    delay: LEASE_CONFLICT_BACKOFF,
                }
            }
            // Lease vanished server-side: drop the ID and reconnect to acquire a fresh lease.
            HeartbeatExit::Gone => {
                retry.previous_lease_id = None;
                WatchAction::Reconnect
            }
            HeartbeatExit::Unauthorized => WatchAction::StopUnauthorized,
            // Repeated heartbeat failures without a terminal status: the lease may still be
            // valid server-side, so carry it through.
            HeartbeatExit::Failed => {
                retry.previous_lease_id = Some(lease_id);
                WatchAction::Reconnect
            }
            HeartbeatExit::Cancelled => WatchAction::Stop,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wake_event() -> WatchWakeEvent {
        WatchWakeEvent {
            remote_access_session_id: Some("ras_test".into()),
            url: "wss://livekit.example".into(),
            token: "lk_token".into(),
        }
    }

    fn lease() -> String {
        "lease-abc".into()
    }

    const HEARTBEAT: Duration = Duration::from_secs(10);
    const HEALTHY: Duration = Duration::from_secs(60);
    const SHORT: Duration = Duration::from_millis(100);

    #[test]
    fn double_backoff_caps_at_max() {
        let mut state = WatchRetryState::new();
        for _ in 0..20 {
            state.double_backoff();
        }
        assert_eq!(state.backoff, MAX_BACKOFF);
    }

    // --- on_connect_success ---

    #[test]
    fn connect_success_clears_lease_and_resets_backoff() {
        let mut state = WatchRetryState {
            previous_lease_id: Some("stale".into()),
            backoff: Duration::from_secs(8),
        };
        on_connect_success(&mut state);
        assert_eq!(state.previous_lease_id, None);
        assert_eq!(state.backoff, INITIAL_BACKOFF);
    }

    // --- on_connect_error ---

    #[test]
    fn connect_error_unauthorized_stops_without_mutating_state() {
        let mut state = WatchRetryState {
            previous_lease_id: Some("keep-me".into()),
            backoff: Duration::from_secs(4),
        };
        let action = on_connect_error(&WatchError::Unauthorized, &mut state);
        assert_eq!(action, ConnectAction::StopUnauthorized);
        // Unauthorized is terminal: state is irrelevant after, but check we didn't disturb it.
        assert_eq!(state.previous_lease_id.as_deref(), Some("keep-me"));
        assert_eq!(state.backoff, Duration::from_secs(4));
    }

    #[test]
    fn connect_error_conflict_drops_lease_and_uses_lease_conflict_backoff() {
        let mut state = WatchRetryState {
            previous_lease_id: Some("ours".into()),
            backoff: Duration::from_secs(2),
        };
        let action = on_connect_error(&WatchError::Conflict, &mut state);
        assert_eq!(action, ConnectAction::RetryAfter(LEASE_CONFLICT_BACKOFF));
        assert_eq!(state.previous_lease_id, None);
        // Conflict uses its own fixed delay and leaves the transient backoff untouched.
        assert_eq!(state.backoff, Duration::from_secs(2));
    }

    #[test]
    fn connect_error_generic_uses_current_backoff_then_doubles() {
        let mut state = WatchRetryState {
            previous_lease_id: Some("keep".into()),
            backoff: Duration::from_secs(2),
        };
        let action = on_connect_error(&WatchError::UnexpectedEof, &mut state);
        assert_eq!(action, ConnectAction::RetryAfter(Duration::from_secs(2)));
        // Lease must be preserved across transient connect failures so the eventual successful
        // reconnect can hand it to the API.
        assert_eq!(state.previous_lease_id.as_deref(), Some("keep"));
        assert_eq!(state.backoff, Duration::from_secs(4));
    }

    #[test]
    fn connect_error_generic_caps_backoff_at_max() {
        let mut state = WatchRetryState {
            previous_lease_id: None,
            backoff: MAX_BACKOFF,
        };
        let action = on_connect_error(&WatchError::HelloTimeout, &mut state);
        assert_eq!(action, ConnectAction::RetryAfter(MAX_BACKOFF));
        assert_eq!(state.backoff, MAX_BACKOFF);
    }

    // --- on_outcome ---

    #[test]
    fn outcome_wake_returns_wake() {
        let mut state = WatchRetryState {
            previous_lease_id: Some("untouched".into()),
            backoff: Duration::from_secs(8),
        };
        let action = on_outcome(
            WatchOutcome::Wake(wake_event()),
            lease(),
            HEALTHY,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Wake(wake_event()));
        // Wake doesn't touch state; the next connect-success will reset it.
        assert_eq!(state.previous_lease_id.as_deref(), Some("untouched"));
        assert_eq!(state.backoff, Duration::from_secs(8));
    }

    #[test]
    fn outcome_read_timeout_reconnects_immediately_with_lease() {
        let mut state = WatchRetryState {
            previous_lease_id: None,
            backoff: Duration::from_secs(8),
        };
        let action = on_outcome(
            WatchOutcome::ReadTimeout,
            lease(),
            SHORT,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Reconnect);
        assert_eq!(state.previous_lease_id, Some(lease()));
        // No backoff change: read-timeout is normal protocol behaviour.
        assert_eq!(state.backoff, Duration::from_secs(8));
    }

    #[test]
    fn outcome_stream_ended_reconnects_immediately_with_lease() {
        let mut state = WatchRetryState::new();
        let action = on_outcome(
            WatchOutcome::StreamEnded,
            lease(),
            SHORT,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Reconnect);
        assert_eq!(state.previous_lease_id, Some(lease()));
        assert_eq!(state.backoff, INITIAL_BACKOFF);
    }

    #[test]
    fn outcome_stream_error_after_healthy_run_reconnects_immediately() {
        let mut state = WatchRetryState {
            previous_lease_id: None,
            backoff: Duration::from_secs(4),
        };
        let action = on_outcome(
            WatchOutcome::StreamError(WatchError::UnexpectedEof),
            lease(),
            HEALTHY,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Reconnect);
        assert_eq!(state.previous_lease_id, Some(lease()));
        // Healthy run: backoff is left to connect_watch — no escalation here.
        assert_eq!(state.backoff, Duration::from_secs(4));
    }

    #[test]
    fn outcome_stream_error_after_short_run_backs_off_and_doubles() {
        let mut state = WatchRetryState {
            previous_lease_id: None,
            backoff: Duration::from_secs(4),
        };
        let action = on_outcome(
            WatchOutcome::StreamError(WatchError::UnexpectedEof),
            lease(),
            SHORT,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(
            action,
            WatchAction::Backoff {
                delay: Duration::from_secs(4),
            }
        );
        assert_eq!(state.previous_lease_id, Some(lease()));
        assert_eq!(state.backoff, Duration::from_secs(8));
    }

    #[test]
    fn outcome_stream_error_short_run_caps_backoff_at_max() {
        let mut state = WatchRetryState {
            previous_lease_id: None,
            backoff: MAX_BACKOFF,
        };
        let action = on_outcome(
            WatchOutcome::StreamError(WatchError::UnexpectedEof),
            lease(),
            SHORT,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Backoff { delay: MAX_BACKOFF });
        assert_eq!(state.backoff, MAX_BACKOFF);
    }

    #[test]
    fn outcome_heartbeat_conflict_drops_lease_with_conflict_backoff() {
        let mut state = WatchRetryState {
            previous_lease_id: Some("ours".into()),
            backoff: Duration::from_secs(8),
        };
        let action = on_outcome(
            WatchOutcome::HeartbeatLost(HeartbeatExit::Conflict),
            lease(),
            HEALTHY,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(
            action,
            WatchAction::Backoff {
                delay: LEASE_CONFLICT_BACKOFF,
            }
        );
        assert_eq!(state.previous_lease_id, None);
        // Conflict on the heartbeat path doesn't escalate transient backoff — the conflict
        // delay is its own thing.
        assert_eq!(state.backoff, Duration::from_secs(8));
    }

    #[test]
    fn outcome_heartbeat_gone_drops_lease_no_delay() {
        let mut state = WatchRetryState {
            previous_lease_id: Some("ours".into()),
            backoff: INITIAL_BACKOFF,
        };
        let action = on_outcome(
            WatchOutcome::HeartbeatLost(HeartbeatExit::Gone),
            lease(),
            HEALTHY,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Reconnect);
        assert_eq!(state.previous_lease_id, None);
    }

    #[test]
    fn outcome_heartbeat_unauthorized_stops() {
        let mut state = WatchRetryState::new();
        let action = on_outcome(
            WatchOutcome::HeartbeatLost(HeartbeatExit::Unauthorized),
            lease(),
            HEALTHY,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::StopUnauthorized);
    }

    #[test]
    fn outcome_heartbeat_failed_keeps_lease_no_delay() {
        let mut state = WatchRetryState::new();
        let action = on_outcome(
            WatchOutcome::HeartbeatLost(HeartbeatExit::Failed),
            lease(),
            HEALTHY,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Reconnect);
        assert_eq!(state.previous_lease_id, Some(lease()));
    }

    #[test]
    fn outcome_heartbeat_cancelled_stops_without_unauthorized() {
        let mut state = WatchRetryState::new();
        let action = on_outcome(
            WatchOutcome::HeartbeatLost(HeartbeatExit::Cancelled),
            lease(),
            HEALTHY,
            HEARTBEAT,
            &mut state,
        );
        assert_eq!(action, WatchAction::Stop);
    }
}
