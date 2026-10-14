use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use bytes::BufMut;
use serde::Serialize;

use crate::remote_access::ConnectionStatus;
use crate::throttler::Throttler;
use crate::{Channel, ChannelBuilder, Context, Encode, Schema};

/// The default topic for remote access gateway statistics. See [`StatsOptions`].
pub const DEFAULT_STATS_TOPIC: &str = "/remote_access_stats";

/// The default interval between remote access gateway statistics messages. See
/// [`StatsOptions`].
pub const DEFAULT_STATS_INTERVAL: Duration = Duration::from_secs(1);

/// Shorter stats intervals are clamped to this.
const MIN_STATS_INTERVAL: Duration = Duration::from_millis(100);

/// Minimum interval between gateway-wide warnings for each warned drop or reset cause.
const WARN_INTERVAL: Duration = Duration::from_secs(30);

/// JSON Schema (draft 2020-12) describing [`RemoteAccessStats`].
const REMOTE_ACCESS_STATS_JSON_SCHEMA: &str = include_str!("stats_schema.json");

/// Options for the statistics that a remote access gateway publishes, enabled with
/// [`Gateway::stats`](crate::remote_access::Gateway::stats).
///
/// The gateway logs a JSON message with the `foxglove.RemoteAccessStats` JSON Schema to a
/// channel in its context, every [`interval`](Self::interval). It keeps publishing while it is
/// dormant or reconnecting, and publishes a last message with the `shutting_down` state when it
/// is stopped. Like any other channel in the context, remote viewers receive it too.
///
/// # Published fields
///
/// Counters are cumulative over the lifetime of the gateway. They are not reset when a session
/// ends or the gateway reconnects.
///
/// | Field | Type | Description |
/// | --- | --- | --- |
/// | `state` | string | `connecting`, `connected`, or `shutting_down`. `connected` means the gateway is available for remote access, whether or not a viewer session is active. |
/// | `session_active` | boolean | Whether the gateway is joined to a remote access session, which happens when a viewer requests a connection. |
/// | `viewer_count` | integer | The number of viewers connected to the current session. |
/// | `rtt_ms` | number | The most recent ICE round-trip time to the remote access server, sampled every 30 seconds. Omitted if no session is active or the session has not been sampled yet. |
/// | `connection_quality` | string | `excellent`, `good`, `poor`, or `lost`. Omitted if no session is active. |
/// | `reconnects` | integer | Times the gateway lost its connection to the Foxglove platform and started reconnecting. |
/// | `participant_resets` | integer | Times a viewer was disconnected and reconnected because its control message queue overflowed or a write to it failed. See [`Gateway::message_backlog_size`](crate::remote_access::Gateway::message_backlog_size). |
/// | `data_track_full` | integer | Messages dropped because a data track's send queue was full. |
/// | `data_track_not_ready` | integer | Messages dropped because a channel's data track was not published, which is expected briefly after each channel is advertised. Not broken down by topic. |
/// | `oversized` | integer | Messages dropped for exceeding [`Gateway::max_data_track_message_size`](crate::remote_access::Gateway::max_data_track_message_size). |
/// | `video_queue` | integer | Image messages dropped before video encoding because the encoder was slower than the log rate. |
/// | `point_cloud_queue` | integer | Point cloud messages dropped before compression because compression was slower than the log rate. |
/// | `video_encode_errors` | integer | Image messages that could not be encoded as video. |
/// | `topics` | array | The drop counters above, except `data_track_not_ready`, for each topic with a nonzero counter, as objects with a `topic` field. |
///
/// Of the drops and resets, only `data_track_full` and `participant_resets` are logged as
/// warnings, throttled across the gateway. The others are expected in normal operation.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsOptions {
    topic: String,
    interval: Duration,
}

impl Default for StatsOptions {
    fn default() -> Self {
        Self {
            topic: DEFAULT_STATS_TOPIC.to_string(),
            interval: DEFAULT_STATS_INTERVAL,
        }
    }
}

impl StatsOptions {
    /// Creates stats options with the default topic and interval.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the topic of the stats channel.
    ///
    /// Each gateway that publishes stats to a context needs its own topic. Defaults to
    /// [`DEFAULT_STATS_TOPIC`].
    pub fn topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = topic.into();
        self
    }

    /// Sets the interval between stats messages.
    ///
    /// The interval is clamped to a minimum of 100 ms. Defaults to [`DEFAULT_STATS_INTERVAL`].
    pub fn interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Creates the stats channel on `context`.
    pub(crate) fn into_publisher(self, context: &Arc<Context>) -> StatsPublisher {
        let interval = if self.interval < MIN_STATS_INTERVAL {
            tracing::info!(
                "remote access stats interval {:?} is below minimum {MIN_STATS_INTERVAL:?}, clamping",
                self.interval
            );
            MIN_STATS_INTERVAL
        } else {
            self.interval
        };
        // An identical existing channel would be shared, and closed when either gateway stops.
        if context.get_channel_by_topic(&self.topic).is_some() {
            tracing::warn!(
                "remote access stats topic {} is already in use in this context; \
                 use a unique stats topic for each gateway",
                self.topic
            );
        }
        let channel = ChannelBuilder::new(self.topic)
            .context(context)
            .build::<RemoteAccessStats>();
        StatsPublisher { channel, interval }
    }
}

/// The stats channel and publishing interval for a gateway with stats enabled.
#[derive(Debug)]
pub(crate) struct StatsPublisher {
    channel: Channel<RemoteAccessStats>,
    interval: Duration,
}

impl StatsPublisher {
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Logs a stats message, which is only built if the channel has sinks.
    pub fn publish(&self, stats: impl FnOnce() -> RemoteAccessStats) {
        if self.channel.has_sinks() {
            self.channel.log(&stats());
        }
    }

    /// Closes the stats channel, removing it from the context.
    pub fn close(&self) {
        self.channel.close();
    }
}

/// The gateway state reported in stats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum StatsState {
    Connecting,
    Connected,
    ShuttingDown,
}

impl From<ConnectionStatus> for StatsState {
    fn from(status: ConnectionStatus) -> Self {
        match status {
            ConnectionStatus::Connecting => Self::Connecting,
            ConnectionStatus::Connected => Self::Connected,
            ConnectionStatus::ShuttingDown | ConnectionStatus::Shutdown => Self::ShuttingDown,
        }
    }
}

/// The remote access server's estimate of the quality of the gateway's connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConnectionQuality {
    Excellent,
    Good,
    Poor,
    Lost,
}

impl From<livekit::prelude::ConnectionQuality> for ConnectionQuality {
    fn from(quality: livekit::prelude::ConnectionQuality) -> Self {
        match quality {
            livekit::prelude::ConnectionQuality::Excellent => Self::Excellent,
            livekit::prelude::ConnectionQuality::Good => Self::Good,
            livekit::prelude::ConnectionQuality::Poor => Self::Poor,
            livekit::prelude::ConnectionQuality::Lost => Self::Lost,
        }
    }
}

/// The stats that describe an active session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SessionSnapshot {
    pub viewer_count: usize,
    pub rtt_ms: Option<f64>,
    pub connection_quality: ConnectionQuality,
}

/// A remote access stats message, encoded as JSON with the `foxglove.RemoteAccessStats` JSON
/// Schema.
#[derive(Debug, Serialize)]
pub(crate) struct RemoteAccessStats {
    state: StatsState,
    session_active: bool,
    viewer_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    rtt_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    connection_quality: Option<ConnectionQuality>,
    reconnects: u64,
    participant_resets: u64,
    data_track_not_ready: u64,
    #[serde(flatten)]
    drops: DropStats,
    topics: Vec<TopicStats>,
}

impl RemoteAccessStats {
    pub fn new(
        status: ConnectionStatus,
        session: Option<SessionSnapshot>,
        counters: &GatewayCounters,
    ) -> Self {
        Self {
            state: status.into(),
            session_active: session.is_some(),
            viewer_count: session.map_or(0, |s| s.viewer_count),
            rtt_ms: session.and_then(|s| s.rtt_ms),
            connection_quality: session.map(|s| s.connection_quality),
            reconnects: counters.reconnects.load(Ordering::Relaxed),
            participant_resets: counters.participant_resets.load(Ordering::Relaxed),
            data_track_not_ready: counters.data_track_not_ready.load(Ordering::Relaxed),
            drops: counters.totals.load(),
            topics: counters.topic_stats(),
        }
    }
}

impl Encode for RemoteAccessStats {
    type Error = serde_json::Error;

    fn get_schema() -> Option<Schema> {
        Some(Schema::new(
            "foxglove.RemoteAccessStats".to_string(),
            "jsonschema".to_string(),
            Cow::Borrowed(REMOTE_ACCESS_STATS_JSON_SCHEMA.as_bytes()),
        ))
    }

    fn get_message_encoding() -> String {
        "json".to_string()
    }

    fn encode(&self, buf: &mut impl BufMut) -> Result<(), Self::Error> {
        serde_json::to_writer(buf.writer(), self)
    }
}

/// Drop counts by cause, for a topic or for the whole gateway.
///
/// Data-track-not-ready drops are counted separately, for the whole gateway only. They happen
/// for a few milliseconds after every advertise, so counting them per topic would list nearly
/// every topic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
struct DropStats {
    data_track_full: u64,
    oversized: u64,
    video_queue: u64,
    point_cloud_queue: u64,
    video_encode_errors: u64,
}

#[derive(Debug, Serialize)]
struct TopicStats {
    topic: String,
    #[serde(flatten)]
    drops: DropStats,
}

#[derive(Debug, Default)]
struct DropCounters {
    data_track_full: AtomicU64,
    oversized: AtomicU64,
    video_queue: AtomicU64,
    point_cloud_queue: AtomicU64,
    video_encode_errors: AtomicU64,
}

impl DropCounters {
    fn load(&self) -> DropStats {
        DropStats {
            data_track_full: self.data_track_full.load(Ordering::Relaxed),
            oversized: self.oversized.load(Ordering::Relaxed),
            video_queue: self.video_queue.load(Ordering::Relaxed),
            point_cloud_queue: self.point_cloud_queue.load(Ordering::Relaxed),
            video_encode_errors: self.video_encode_errors.load(Ordering::Relaxed),
        }
    }
}

/// Gateway-lifetime counters, shared by every session the gateway runs.
///
/// Counting is always on, so that the counts are the same whether or not stats are published.
#[derive(Debug)]
pub(crate) struct GatewayCounters {
    totals: DropCounters,
    /// Per-topic drop counters, added when a topic first counts a drop.
    topics: parking_lot::Mutex<BTreeMap<String, Arc<DropCounters>>>,
    data_track_not_ready: AtomicU64,
    reconnects: AtomicU64,
    participant_resets: AtomicU64,
    data_track_full_throttler: parking_lot::Mutex<Throttler>,
    participant_reset_throttler: parking_lot::Mutex<Throttler>,
}

impl Default for GatewayCounters {
    fn default() -> Self {
        Self {
            totals: DropCounters::default(),
            topics: parking_lot::Mutex::default(),
            data_track_not_ready: AtomicU64::new(0),
            reconnects: AtomicU64::new(0),
            participant_resets: AtomicU64::new(0),
            data_track_full_throttler: parking_lot::Mutex::new(Throttler::new(WARN_INTERVAL)),
            participant_reset_throttler: parking_lot::Mutex::new(Throttler::new(WARN_INTERVAL)),
        }
    }
}

impl GatewayCounters {
    /// Returns the drop counters for `topic`.
    pub fn for_topic(self: &Arc<Self>, topic: &str) -> TopicCounters {
        TopicCounters {
            gateway: self.clone(),
            topic: topic.into(),
            counters: OnceLock::new(),
        }
    }

    pub fn record_reconnect(&self) {
        self.reconnects.fetch_add(1, Ordering::Relaxed);
    }

    /// Counts a participant reset. Returns the cumulative count if a warning is due, which
    /// happens at most once per [`WARN_INTERVAL`] across the gateway.
    #[must_use]
    pub fn record_participant_reset(&self) -> Option<u64> {
        let total = self.participant_resets.fetch_add(1, Ordering::Relaxed) + 1;
        self.participant_reset_throttler
            .lock()
            .try_acquire()
            .then_some(total)
    }

    fn topic_stats(&self) -> Vec<TopicStats> {
        self.topics
            .lock()
            .iter()
            .map(|(topic, counters)| TopicStats {
                topic: topic.clone(),
                drops: counters.load(),
            })
            .filter(|stats| stats.drops != DropStats::default())
            .collect()
    }
}

#[cfg(test)]
impl GatewayCounters {
    /// Returns a stats message with these counters, and no session, as JSON.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(RemoteAccessStats::new(
            ConnectionStatus::Connected,
            None,
            self,
        ))
        .expect("stats serialize as JSON")
    }
}

/// Drop counters for one topic, which also count toward the gateway-wide totals.
#[derive(Debug, Clone)]
pub(crate) struct TopicCounters {
    gateway: Arc<GatewayCounters>,
    topic: Arc<str>,
    /// Registered with the gateway on the first drop, so that only topics with drops are
    /// tracked.
    counters: OnceLock<Arc<DropCounters>>,
}

impl TopicCounters {
    /// Adds `count` to a counter for this topic and the gateway, returning the gateway total.
    fn add(&self, counter: fn(&DropCounters) -> &AtomicU64, count: u64) -> u64 {
        let topic_counters = self.counters.get_or_init(|| {
            self.gateway
                .topics
                .lock()
                .entry(self.topic.to_string())
                .or_default()
                .clone()
        });
        counter(topic_counters).fetch_add(count, Ordering::Relaxed);
        counter(&self.gateway.totals).fetch_add(count, Ordering::Relaxed) + count
    }

    /// Counts a data-track-not-ready drop, for the whole gateway only.
    pub fn record_data_track_not_ready_drop(&self) {
        self.gateway
            .data_track_not_ready
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Counts a data-track-full drop. Returns the gateway-wide cumulative count if a warning is
    /// due, which happens at most once per [`WARN_INTERVAL`] across the gateway.
    #[must_use]
    pub fn record_data_track_full_drop(&self) -> Option<u64> {
        let total = self.add(|c| &c.data_track_full, 1);
        self.gateway
            .data_track_full_throttler
            .lock()
            .try_acquire()
            .then_some(total)
    }

    pub fn record_oversized_drop(&self) {
        self.add(|c| &c.oversized, 1);
    }

    pub fn record_video_queue_drops(&self, count: u64) {
        if count > 0 {
            self.add(|c| &c.video_queue, count);
        }
    }

    pub fn record_point_cloud_queue_drops(&self, count: u64) {
        if count > 0 {
            self.add(|c| &c.point_cloud_queue, count);
        }
    }

    pub fn record_video_encode_error(&self) {
        self.add(|c| &c.video_encode_errors, 1);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;

    fn to_json(stats: &RemoteAccessStats) -> serde_json::Value {
        let mut buf = Vec::new();
        stats.encode(&mut buf).expect("stats encode");
        serde_json::from_slice(&buf).expect("stats encode as JSON")
    }

    fn keys(value: &serde_json::Value) -> BTreeSet<String> {
        value
            .as_object()
            .expect("value is a JSON object")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn counts_drops_per_topic_and_in_gateway_totals() {
        let counters = Arc::new(GatewayCounters::default());
        let imu = counters.for_topic("/imu");
        let camera = counters.for_topic("/camera");
        let _quiet = counters.for_topic("/quiet");
        let advertised = counters.for_topic("/advertised");

        imu.record_data_track_not_ready_drop();
        imu.record_data_track_not_ready_drop();
        let _ = imu.record_data_track_full_drop();
        // Another handle for the same topic shares its counters.
        counters.for_topic("/imu").record_oversized_drop();
        camera.record_data_track_not_ready_drop();
        advertised.record_data_track_not_ready_drop();
        camera.record_video_queue_drops(4);
        camera.record_video_queue_drops(0);
        camera.record_video_encode_error();
        counters
            .for_topic("/lidar")
            .record_point_cloud_queue_drops(5);
        counters.record_reconnect();
        let _ = counters.record_participant_reset();

        let stats = to_json(&RemoteAccessStats::new(
            ConnectionStatus::Connected,
            None,
            &counters,
        ));
        assert_eq!(
            stats,
            json!({
                "state": "connected",
                "session_active": false,
                "viewer_count": 0,
                "reconnects": 1,
                "participant_resets": 1,
                "data_track_full": 1,
                "data_track_not_ready": 4,
                "oversized": 1,
                "video_queue": 4,
                "point_cloud_queue": 5,
                "video_encode_errors": 1,
                // Sorted by topic. Not-ready drops are only counted gateway-wide, so topics
                // with no other drops, like /advertised, are not listed.
                "topics": [
                    {
                        "topic": "/camera",
                        "data_track_full": 0,
                        "oversized": 0,
                        "video_queue": 4,
                        "point_cloud_queue": 0,
                        "video_encode_errors": 1,
                    },
                    {
                        "topic": "/imu",
                        "data_track_full": 1,
                        "oversized": 1,
                        "video_queue": 0,
                        "point_cloud_queue": 0,
                        "video_encode_errors": 0,
                    },
                    {
                        "topic": "/lidar",
                        "data_track_full": 0,
                        "oversized": 0,
                        "video_queue": 0,
                        "point_cloud_queue": 5,
                        "video_encode_errors": 0,
                    },
                ],
            })
        );
    }

    #[test]
    fn warned_causes_are_throttled_but_always_counted() {
        let counters = Arc::new(GatewayCounters::default());
        let imu = counters.for_topic("/imu");
        let camera = counters.for_topic("/camera");

        // The first event of each warned cause is due a warning; later ones within the
        // interval are counted silently. Track-full warnings are throttled across topics.
        assert_eq!(imu.record_data_track_full_drop(), Some(1));
        assert_eq!(imu.record_data_track_full_drop(), None);
        assert_eq!(camera.record_data_track_full_drop(), None);
        assert_eq!(counters.record_participant_reset(), Some(1));
        assert_eq!(counters.record_participant_reset(), None);

        let stats = to_json(&RemoteAccessStats::new(
            ConnectionStatus::Connected,
            None,
            &counters,
        ));
        assert_eq!(stats["data_track_full"], 3);
        assert_eq!(stats["participant_resets"], 2);
    }

    #[test]
    fn reports_session_fields_only_while_a_session_is_active() {
        let counters = GatewayCounters::default();
        let session = SessionSnapshot {
            viewer_count: 2,
            rtt_ms: Some(12.5),
            connection_quality: ConnectionQuality::Good,
        };

        let active = to_json(&RemoteAccessStats::new(
            ConnectionStatus::Connected,
            Some(session),
            &counters,
        ));
        assert_eq!(active["session_active"], true);
        assert_eq!(active["viewer_count"], 2);
        assert_eq!(active["rtt_ms"], 12.5);
        assert_eq!(active["connection_quality"], "good");

        let unsampled = to_json(&RemoteAccessStats::new(
            ConnectionStatus::Connected,
            Some(SessionSnapshot {
                rtt_ms: None,
                ..session
            }),
            &counters,
        ));
        assert!(unsampled.get("rtt_ms").is_none());
        assert_eq!(unsampled["connection_quality"], "good");

        let dormant = to_json(&RemoteAccessStats::new(
            ConnectionStatus::Connected,
            None,
            &counters,
        ));
        assert_eq!(dormant["session_active"], false);
        assert_eq!(dormant["viewer_count"], 0);
        assert!(dormant.get("rtt_ms").is_none());
        assert!(dormant.get("connection_quality").is_none());
    }

    #[test]
    fn schema_properties_match_serialized_fields() {
        // Guards against drift between the hand-written JSON schema and the serialized
        // message, including the optional fields and the per-topic objects.
        let counters = Arc::new(GatewayCounters::default());
        counters.for_topic("/imu").record_oversized_drop();
        let stats = to_json(&RemoteAccessStats::new(
            ConnectionStatus::Connected,
            Some(SessionSnapshot {
                viewer_count: 1,
                rtt_ms: Some(1.0),
                connection_quality: ConnectionQuality::Excellent,
            }),
            &counters,
        ));

        let schema = RemoteAccessStats::get_schema().expect("RemoteAccessStats has a schema");
        assert_eq!(schema.name, "foxglove.RemoteAccessStats");
        assert_eq!(schema.encoding, "jsonschema");
        assert_eq!(RemoteAccessStats::get_message_encoding(), "json");
        let schema: serde_json::Value =
            serde_json::from_slice(&schema.data).expect("schema is valid JSON");

        assert_eq!(
            keys(&schema["properties"]),
            keys(&stats),
            "JSON schema properties must match RemoteAccessStats serialized field names"
        );
        assert_eq!(
            keys(&schema["properties"]["topics"]["items"]["properties"]),
            keys(&stats["topics"][0]),
            "JSON schema topic properties must match TopicStats serialized field names"
        );
    }

    #[test]
    fn publisher_creates_channel_and_clamps_interval() {
        let ctx = Context::new();
        let publisher = StatsOptions::new()
            .topic("/custom/stats")
            .interval(Duration::ZERO)
            .into_publisher(&ctx);
        assert_eq!(publisher.interval(), MIN_STATS_INTERVAL);

        let channel = ctx
            .get_channel_by_topic("/custom/stats")
            .expect("stats channel is registered");
        assert_eq!(channel.message_encoding(), "json");
        assert_eq!(
            channel.schema().map(|s| s.name.as_str()),
            Some("foxglove.RemoteAccessStats")
        );

        publisher.close();
        assert!(ctx.get_channel_by_topic("/custom/stats").is_none());
    }
}
