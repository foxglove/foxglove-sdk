//! Long-lived SSE watch stream used by a dormant remote access gateway.
//!
//! The watch stream is opened against the Foxglove platform API. The server emits a `hello`
//! event with a server-generated lease ID and control-loop timeouts, followed by zero or more
//! heartbeat comments and eventually a single `wake` event carrying LiveKit credentials. While
//! the stream is open, the gateway POSTs periodic heartbeats to prove liveness and refresh its
//! lease.

use std::{pin::Pin, sync::Arc, time::Duration};

use futures_util::{Stream, StreamExt};
use reqwest::StatusCode;
use thiserror::Error;
use tokio::{
    sync::oneshot,
    task::JoinHandle,
    time::{Instant, MissedTickBehavior},
};
use tracing::{debug, error, info, warn};

use crate::api_client::{
    DeviceToken, FoxgloveApiClient, FoxgloveApiClientError, WatchHelloEvent, WatchQuery,
    WatchWakeEvent,
};

use super::sse::{SseFrame, sse_event_stream};

/// Timeout applied from opening a watch request until a `hello` event has been observed.
const HELLO_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum consecutive heartbeat silence tolerated before forcing a watch reconnect.
const MAX_MISSED_HEARTBEAT_INTERVALS: u32 = 3;

/// Minimum heartbeat interval.
const MIN_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

/// Minimum device-wait-for-viewer duration.
const MIN_DEVICE_WAIT_FOR_VIEWER: Duration = Duration::from_secs(5);

/// Errors produced while establishing or reading from a watch stream.
#[derive(Error, Debug)]
#[non_exhaustive]
pub(super) enum WatchError {
    /// The handler rejected the connection because another gateway already holds the lease.
    #[error("watch stream conflict: another gateway holds the lease")]
    Conflict,

    /// The device token was rejected by the API.
    #[error("device token unauthorized")]
    Unauthorized,

    /// HTTP or network-level error opening the stream.
    #[error(transparent)]
    Api(#[from] FoxgloveApiClientError),

    /// Error reading bytes from an established stream body.
    #[error("stream transport error: {0}")]
    Transport(#[source] reqwest::Error),

    /// The stream ended before we observed a usable `hello` event.
    #[error("watch stream ended before `hello` event")]
    UnexpectedEof,

    /// The `hello` handshake did not arrive within [`HELLO_TIMEOUT`].
    #[error("timed out waiting for `hello` event")]
    HelloTimeout,

    /// Received an event we could not parse.
    #[error("malformed `{event}` event: {source}")]
    MalformedEvent {
        event: &'static str,
        #[source]
        source: serde_json::Error,
    },
}

/// Reason a heartbeat task exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeartbeatExit {
    /// Server returned 409 — another gateway holds the lease.
    Conflict,
    /// Server returned 410 — no active lease exists for this device.
    Gone,
    /// Server returned 401 — the device token is not accepted.
    Unauthorized,
    /// Heartbeats failed for too long without a successful refresh.
    Failed,
    /// The heartbeat task sender was dropped before it reported a terminal reason.
    Cancelled,
}

/// Outcome of a single `Watch::run()` call. Each variant is terminal — once
/// observed, the caller should stop using this watch and either close it or take an
/// appropriate control-loop action.
#[derive(Debug)]
pub(super) enum WatchOutcome {
    /// Received a `wake` event. Ready to join LiveKit.
    Wake(WatchWakeEvent),
    /// The SSE stream closed cleanly without a wake.
    StreamEnded,
    /// No server-sent frame arrived within the SSE read-timeout window (see
    /// [`Watch::run`]).
    ReadTimeout,
    /// A transport error occurred while reading the stream.
    StreamError(WatchError),
    /// The heartbeat task exited abnormally.
    HeartbeatLost(HeartbeatExit),
}

/// A connected watch. Spawns an internal heartbeat task on construction and aborts it
/// on [`close`](Self::close) (or on drop as a safety net).
pub(super) struct Watch {
    lease_id: String,
    heartbeat_interval: Duration,
    device_wait_for_viewer: Duration,
    events: Pin<Box<dyn Stream<Item = Result<SseFrame, reqwest::Error>> + Send>>,
    heartbeat_handle: Option<JoinHandle<()>>,
    heartbeat_exit: oneshot::Receiver<HeartbeatExit>,
}

impl Watch {
    /// Opens the watch stream and waits for the initial `hello` event.
    pub async fn connect(
        client: Arc<FoxgloveApiClient<DeviceToken>>,
        query: WatchQuery,
    ) -> Result<Self, WatchError> {
        Watch::connect_inner(client, query)
            .await
            .inspect_err(|e| warn!(error=%e, "watch stream connect failed"))
    }

    async fn connect_inner(
        client: Arc<FoxgloveApiClient<DeviceToken>>,
        query: WatchQuery,
    ) -> Result<Self, WatchError> {
        let hello_deadline = Instant::now() + HELLO_TIMEOUT;
        let Ok(response) =
            tokio::time::timeout_at(hello_deadline, client.open_watch_stream(&query)).await
        else {
            return Err(WatchError::HelloTimeout);
        };
        let response = match response {
            Ok(response) => response,
            Err(e) => {
                if e.status_code() == Some(StatusCode::CONFLICT) {
                    return Err(WatchError::Conflict);
                }
                if e.status_code() == Some(StatusCode::UNAUTHORIZED) {
                    return Err(WatchError::Unauthorized);
                }
                return Err(WatchError::Api(e));
            }
        };

        let mut events = sse_event_stream(response.bytes_stream());

        // Read until we see a `hello` (ignoring comments and stray events).
        let hello = loop {
            let frame = match tokio::time::timeout_at(hello_deadline, events.next()).await {
                Err(_) => return Err(WatchError::HelloTimeout),
                Ok(None) => return Err(WatchError::UnexpectedEof),
                Ok(Some(Err(e))) => return Err(WatchError::Transport(e)),
                Ok(Some(Ok(frame))) => frame,
            };
            let event = match frame {
                SseFrame::Comment => continue,
                SseFrame::Event(event) => event,
            };
            if event.event == "hello" {
                let hello: WatchHelloEvent =
                    serde_json::from_str(&event.data).map_err(|e| WatchError::MalformedEvent {
                        event: "hello",
                        source: e,
                    })?;
                break hello;
            }
            debug!(event = %event.event, "ignoring unexpected event before hello");
        };

        // Clamp durations to reasonable values.
        let heartbeat_interval =
            Duration::from_millis(hello.heartbeat_interval_ms).max(MIN_HEARTBEAT_INTERVAL);
        let device_wait_for_viewer =
            Duration::from_millis(hello.device_wait_for_viewer_ms).max(MIN_DEVICE_WAIT_FOR_VIEWER);

        info!(
            watch_lease_id = &hello.watch_lease_id,
            ?heartbeat_interval,
            ?device_wait_for_viewer,
            "watch stream established"
        );

        let (exit_tx, exit_rx) = oneshot::channel::<HeartbeatExit>();
        let heartbeat_handle = tokio::spawn(heartbeat_task(
            client.clone(),
            hello.watch_lease_id.clone(),
            heartbeat_interval,
            exit_tx,
        ));

        Ok(Self {
            lease_id: hello.watch_lease_id,
            heartbeat_interval,
            device_wait_for_viewer,
            events,
            heartbeat_handle: Some(heartbeat_handle),
            heartbeat_exit: exit_rx,
        })
    }

    pub fn lease_id(&self) -> &str {
        &self.lease_id
    }

    pub fn device_wait_for_viewer(&self) -> Duration {
        self.device_wait_for_viewer
    }

    /// Runs the watch session, logs the outcome, closes the watch, and returns the outcome.
    pub async fn run(mut self) -> WatchOutcome {
        let outcome = self.run_inner().await;
        log_watch_outcome(&outcome, &self.lease_id);
        self.close().await;
        outcome
    }

    async fn run_inner(&mut self) -> WatchOutcome {
        // The RFC requires a read-timeout of at least `2 * heartbeat_interval_ms` so that one
        // missed wire-heartbeat does not falsely trip a reconnect. Add a half-interval cushion
        // to absorb scheduling jitter at the boundary.
        let read_timeout = self
            .heartbeat_interval
            .saturating_mul(2)
            .saturating_add(self.heartbeat_interval / 2);
        let events = &mut self.events;
        let heartbeat_exit = &mut self.heartbeat_exit;
        loop {
            tokio::select! {
                biased;
                hb_exit = &mut *heartbeat_exit => {
                    return match hb_exit {
                        Ok(reason) => WatchOutcome::HeartbeatLost(reason),
                        Err(_) => WatchOutcome::HeartbeatLost(HeartbeatExit::Cancelled),
                    };
                },
                ev = tokio::time::timeout(read_timeout, events.next()) => match ev {
                    Err(_) => return WatchOutcome::ReadTimeout,
                    Ok(None) => return WatchOutcome::StreamEnded,
                    Ok(Some(Err(e))) => {
                        return WatchOutcome::StreamError(WatchError::Transport(e));
                    }
                    Ok(Some(Ok(SseFrame::Comment))) => {
                        // Wire-heartbeat from the server: any byte counts as proof of life and
                        // resets the read-timeout on the next iteration.
                        continue;
                    }
                    Ok(Some(Ok(SseFrame::Event(event)))) => match event.event.as_str() {
                        "wake" => match serde_json::from_str::<WatchWakeEvent>(&event.data) {
                            Ok(wake) => return WatchOutcome::Wake(wake),
                            Err(e) => return WatchOutcome::StreamError(WatchError::MalformedEvent {
                                event: "wake",
                                source: e,
                            }),
                        },
                        "hello" => {
                            warn!("received unexpected `hello` event on open stream; ignoring");
                            continue;
                        }
                        other => {
                            debug!(event = %other, "ignoring unknown SSE event");
                            continue;
                        }
                    },
                },
            };
        }
    }

    /// Stops the heartbeat task and waits until it has exited. This satisfies the invariant that
    /// "the gateway must stop heartbeating that lease before reconnecting".
    async fn close(mut self) {
        if let Some(handle) = self.heartbeat_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        // Safety net: abort the heartbeat task if close() wasn't called.
        if let Some(handle) = self.heartbeat_handle.take() {
            handle.abort();
        }
    }
}

/// Heartbeat task. Refreshes the lease every `interval` and exits when it receives a terminal
/// response from the API or when successful refreshes have been absent for too many heartbeat
/// intervals. The owning [`Watch`] aborts this task during close/drop.
async fn heartbeat_task(
    client: Arc<FoxgloveApiClient<DeviceToken>>,
    lease_id: String,
    interval: Duration,
    exit_tx: oneshot::Sender<HeartbeatExit>,
) {
    let max_heartbeat_silence = interval.saturating_mul(MAX_MISSED_HEARTBEAT_INTERVALS);
    let mut last_success = Instant::now();
    let mut ticker = tokio::time::interval_at(Instant::now() + interval, interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let result = client.post_watch_heartbeat(&lease_id).await;
        match result {
            Ok(()) => {
                last_success = Instant::now();
            }
            Err(e) => match e.status_code() {
                Some(StatusCode::CONFLICT) => {
                    let _ = exit_tx.send(HeartbeatExit::Conflict);
                    return;
                }
                Some(StatusCode::GONE) => {
                    let _ = exit_tx.send(HeartbeatExit::Gone);
                    return;
                }
                Some(StatusCode::UNAUTHORIZED) => {
                    let _ = exit_tx.send(HeartbeatExit::Unauthorized);
                    return;
                }
                _ => {
                    if last_success.elapsed() >= max_heartbeat_silence {
                        warn!(
                            error = %e,
                            stale_for_ms = last_success.elapsed().as_millis(),
                            "heartbeat failed for too long"
                        );
                        let _ = exit_tx.send(HeartbeatExit::Failed);
                        return;
                    }
                    debug!(error = %e, "heartbeat failed; will retry");
                }
            },
        }
    }
}

/// Logs a [`WatchOutcome`].
fn log_watch_outcome(outcome: &WatchOutcome, watch_lease_id: &str) {
    match outcome {
        WatchOutcome::Wake(wake) => info!(
            watch_lease_id,
            remote_access_session_id = wake.remote_access_session_id.as_deref(),
            "received wake"
        ),
        WatchOutcome::ReadTimeout => {
            warn!(watch_lease_id, "watch stream read-timeout; reconnecting")
        }
        WatchOutcome::StreamEnded => warn!(
            watch_lease_id,
            "watch stream ended before wake; reconnecting"
        ),
        WatchOutcome::StreamError(e) => warn!(
            watch_lease_id,
            error = %e,
            "watch stream error; reconnecting"
        ),
        WatchOutcome::HeartbeatLost(reason) => match reason {
            HeartbeatExit::Conflict => warn!(
                watch_lease_id,
                "another gateway holds the watch lease; backing off"
            ),
            HeartbeatExit::Gone => warn!(
                watch_lease_id,
                "watch lease gone; reconnecting to acquire a fresh lease"
            ),
            HeartbeatExit::Unauthorized => error!(
                watch_lease_id,
                "device token unauthorized; stopping remote access gateway"
            ),
            HeartbeatExit::Failed => warn!(
                watch_lease_id,
                "watch heartbeat failed for too long; reconnecting"
            ),
            // The heartbeat task only drops its sender without sending if it panicked or was
            // externally aborted. Neither happens in normal operation; if it fires in production,
            // it indicates a bug.
            HeartbeatExit::Cancelled => error!(
                watch_lease_id,
                "heartbeat task exited without a terminal reason; check for panics"
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::future;
    use std::task::{Context, Poll};

    use assert_matches::assert_matches;
    use axum::extract::State;
    use axum::http::{StatusCode as AxumStatusCode, header};
    use axum::response::IntoResponse;
    use axum::{Router, routing::get};
    use futures_util::stream;
    use tokio::net::TcpListener;

    use crate::api_client::{FoxgloveApiClientBuilder, WatchQuery};
    use crate::remote_access::sse::{SseEvent, SseFrame};

    use super::*;

    const TEST_TOKEN: &str = "fox_dt_testtoken";

    #[derive(Clone)]
    struct WatchResponse {
        status: AxumStatusCode,
        body: &'static str,
    }

    struct WatchServer {
        url: String,
        join_handle: JoinHandle<()>,
    }

    impl WatchServer {
        fn url(&self) -> &str {
            &self.url
        }
    }

    impl Drop for WatchServer {
        fn drop(&mut self) {
            self.join_handle.abort();
        }
    }

    struct NotifyOnDrop(Option<oneshot::Sender<()>>);

    impl Drop for NotifyOnDrop {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }

    impl future::Future for NotifyOnDrop {
        type Output = ();

        fn poll(self: std::pin::Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }

    async fn watch_handler(State(response): State<WatchResponse>) -> impl IntoResponse {
        (
            response.status,
            [(header::CONTENT_TYPE, "text/event-stream")],
            response.body,
        )
    }

    async fn watch_server(response: WatchResponse) -> WatchServer {
        let app = Router::new()
            .route(
                "/internal/platform/v1/remote-sessions/watch",
                get(watch_handler),
            )
            .with_state(response);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let join_handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        WatchServer {
            url: format!("http://{addr}"),
            join_handle,
        }
    }

    fn test_client(server: &WatchServer) -> Arc<FoxgloveApiClient<DeviceToken>> {
        Arc::new(
            FoxgloveApiClientBuilder::new(DeviceToken::new(TEST_TOKEN))
                .base_url(server.url())
                .timeout(Duration::from_millis(100))
                .build()
                .unwrap(),
        )
    }

    fn event(event: &str, data: &str) -> SseFrame {
        SseFrame::Event(SseEvent {
            event: event.to_string(),
            data: data.to_string(),
        })
    }

    fn watch_from_stream<S>(
        events: S,
        heartbeat_interval_ms: u64,
    ) -> (Watch, oneshot::Sender<HeartbeatExit>)
    where
        S: Stream<Item = Result<SseFrame, reqwest::Error>> + Send + 'static,
    {
        let (exit_tx, exit_rx) = oneshot::channel();
        let watch = Watch {
            lease_id: "lease-1".to_string(),
            device_wait_for_viewer: Duration::from_secs(45),
            heartbeat_interval: Duration::from_millis(heartbeat_interval_ms),
            events: Box::pin(events),
            heartbeat_handle: None,
            heartbeat_exit: exit_rx,
        };
        (watch, exit_tx)
    }

    fn watch_from_frames(
        frames: Vec<SseFrame>,
        heartbeat_interval_ms: u64,
    ) -> (Watch, oneshot::Sender<HeartbeatExit>) {
        watch_from_stream(
            stream::iter(frames.into_iter().map(Ok::<SseFrame, reqwest::Error>)),
            heartbeat_interval_ms,
        )
    }

    #[tokio::test]
    async fn connect_reads_hello_after_non_hello_frames() {
        let server = watch_server(WatchResponse {
            status: AxumStatusCode::OK,
            body: concat!(
                ": keepalive\n",
                "event: ignored\n",
                "data: {}\n\n",
                "event: hello\n",
                "data: {\"watchLeaseId\":\"lease-1\",\"deviceWaitForViewerMs\":45000,\"heartbeatIntervalMs\":60000}\n\n",
            ),
        })
        .await;
        let client = test_client(&server);

        let watch = Watch::connect(client, WatchQuery::default())
            .await
            .expect("watch should connect");

        assert_eq!(watch.lease_id(), "lease-1");
        assert_eq!(watch.device_wait_for_viewer, Duration::from_secs(45));
        assert_eq!(watch.heartbeat_interval, Duration::from_secs(60));
        watch.close().await;
    }

    #[tokio::test]
    async fn connect_maps_conflict_status() {
        let server = watch_server(WatchResponse {
            status: AxumStatusCode::CONFLICT,
            body: "{\"error\":\"lease already held\"}",
        })
        .await;
        let client = test_client(&server);

        let result = Watch::connect(client, WatchQuery::default()).await;

        assert!(matches!(result, Err(WatchError::Conflict)));
    }

    #[tokio::test]
    async fn connect_maps_unauthorized_status() {
        let server = watch_server(WatchResponse {
            status: AxumStatusCode::UNAUTHORIZED,
            body: "{\"error\":\"invalid device token\"}",
        })
        .await;
        let client = test_client(&server);

        let result = Watch::connect(client, WatchQuery::default()).await;

        assert!(matches!(result, Err(WatchError::Unauthorized)));
    }

    #[tokio::test]
    async fn connect_clamps_durations() {
        let server = watch_server(WatchResponse {
            status: AxumStatusCode::OK,
            body: concat!(
                "event: hello\n",
                "data: {\"watchLeaseId\":\"lease-1\",\"deviceWaitForViewerMs\":1,\"heartbeatIntervalMs\":2}\n\n",
            ),
        })
        .await;
        let client = test_client(&server);

        let watch = Watch::connect(client, WatchQuery::default())
            .await
            .expect("watch should connect");

        assert_eq!(watch.device_wait_for_viewer, MIN_DEVICE_WAIT_FOR_VIEWER);
        assert_eq!(watch.heartbeat_interval, MIN_HEARTBEAT_INTERVAL);
        watch.close().await;
    }

    #[tokio::test]
    async fn watch_ignores_non_terminal_frames_until_wake() {
        let wake_json = "{\"remoteAccessSessionId\":\"ras-1\",\"url\":\"wss://example.test\",\"token\":\"token-1\"}";
        let (watch, _exit_tx) = watch_from_frames(
            vec![
                SseFrame::Comment,
                event("hello", "{}"),
                event("ignored", "{}"),
                event("wake", wake_json),
            ],
            60_000,
        );

        let outcome = watch.run().await;

        let WatchOutcome::Wake(wake) = outcome else {
            panic!("expected wake outcome");
        };
        assert_eq!(wake.remote_access_session_id.as_deref(), Some("ras-1"));
        assert_eq!(wake.url, "wss://example.test");
        assert_eq!(wake.token, "token-1");
    }

    #[tokio::test]
    async fn watch_reports_malformed_wake() {
        let (watch, _exit_tx) = watch_from_frames(vec![event("wake", "{")], 60_000);

        let outcome = watch.run().await;

        assert_matches!(
            outcome,
            WatchOutcome::StreamError(WatchError::MalformedEvent { event: "wake", .. })
        );
    }

    #[tokio::test]
    async fn watch_reports_stream_end() {
        let (watch, _exit_tx) = watch_from_frames(Vec::new(), 60_000);

        let outcome = watch.run().await;

        assert_matches!(outcome, WatchOutcome::StreamEnded);
    }

    #[tokio::test]
    async fn watch_reports_read_timeout() {
        let (watch, _exit_tx) =
            watch_from_stream(stream::pending::<Result<SseFrame, reqwest::Error>>(), 1);

        let outcome = tokio::time::timeout(Duration::from_secs(1), watch.run())
            .await
            .expect("watch read timeout should fire");

        assert_matches!(outcome, WatchOutcome::ReadTimeout);
    }

    #[tokio::test]
    async fn watch_prefers_heartbeat_exit() {
        let wake_json = "{\"url\":\"wss://example.test\",\"token\":\"token-1\"}";
        let (watch, exit_tx) = watch_from_frames(vec![event("wake", wake_json)], 60_000);
        exit_tx.send(HeartbeatExit::Gone).unwrap();

        let outcome = watch.run().await;

        assert_matches!(outcome, WatchOutcome::HeartbeatLost(HeartbeatExit::Gone));
    }

    #[tokio::test]
    async fn close_aborts_heartbeat_task() {
        let (dropped_tx, dropped_rx) = oneshot::channel();
        let heartbeat_handle = tokio::spawn(NotifyOnDrop(Some(dropped_tx)));
        let (_exit_tx, exit_rx) = oneshot::channel();

        let watch = Watch {
            lease_id: "lease-1".to_string(),
            device_wait_for_viewer: Duration::from_secs(45),
            heartbeat_interval: Duration::from_secs(60),
            events: Box::pin(stream::pending::<Result<SseFrame, reqwest::Error>>()),
            heartbeat_handle: Some(heartbeat_handle),
            heartbeat_exit: exit_rx,
        };

        watch.close().await;

        dropped_rx
            .await
            .expect("heartbeat future should be dropped after close");
    }
}
