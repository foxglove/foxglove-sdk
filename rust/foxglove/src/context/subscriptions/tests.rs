use std::sync::Arc;

use crate::testutil::MockSink;
use crate::{ChannelId, Sink};

use super::Subscriptions;

macro_rules! assert_subscribers_eq {
    ($left:expr, $right:expr) => {
        assert_subscribers_eq!($left, $right,);
    };
    ($left:expr, $right:expr, $($arg:tt),*) => {
        let mut left: Vec<_> = $left.into_iter().map(|sink| sink.id()).collect();
        left.sort_unstable();
        let right: Vec<_> = $right.into_iter().collect();
        assert_eq!(left, right, $($arg),*);
    };
}

fn chid(id: u64) -> ChannelId {
    ChannelId::new(id)
}

#[test]
fn test_subscriptions() {
    let s1 = Arc::new(MockSink::default()) as Arc<dyn Sink>;
    let s2 = Arc::new(MockSink::default()) as Arc<dyn Sink>;
    let s3 = Arc::new(MockSink::default()) as Arc<dyn Sink>;

    let mut subs = Subscriptions::default();
    assert_subscribers_eq!(subs.get_subscribers(chid(99)), []);

    // Per-topic subscriptions.
    assert!(subs.subscribe_channels(&s1, &[chid(1), chid(2)]));
    assert!(subs.subscribe_channels(&s2, &[chid(2), chid(3)]));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s1.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s1.id(), s2.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(3)), [s2.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(99)), []);

    // Global subscription.
    assert!(subs.subscribe_global(s3.clone()));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s1.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s1.id(), s2.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(3)), [s2.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(99)), [s3.id()]);

    // Add a per-topic subscription for an existing global subscriber. This should be a no-op. The
    // subscriber should only appear once in the set of subscribers for the topic.
    assert!(!subs.subscribe_channels(&s3, &[chid(3)]));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s1.id(), s3.id()]);

    // Removing a topic subscription for a global subscriber is a no-op, and doesn't remove the
    // global subscription.
    assert!(!subs.unsubscribe_channels(s3.id(), &[chid(3)]));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s1.id(), s3.id()]);

    // Unsubscribe from a particular topic.
    assert!(subs.unsubscribe_channels(s1.id(), &[chid(1)]));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s1.id(), s2.id(), s3.id()]);

    // Unsubscribe from multiple topics. Unsubscribe is idempotent.
    assert!(subs.unsubscribe_channels(s1.id(), &[chid(1), chid(2)]));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s2.id(), s3.id()]);
    assert!(!subs.unsubscribe_channels(s1.id(), &[chid(2)]));

    // Add a global subscription after a per-topic subscription. No duplicate subscribers!
    assert!(subs.subscribe_channels(&s1, &[chid(1)]));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s1.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s2.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(3)), [s2.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(99)), [s3.id()]);
    assert!(subs.subscribe_global(s1.clone()));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s1.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s1.id(), s2.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(3)), [s1.id(), s2.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(99)), [s1.id(), s3.id()]);

    // Remove per-channel subscriptions for channel 2, leaving only the global subscribers.
    assert!(subs.remove_channel_subscriptions(chid(2)));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s1.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s1.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(3)), [s1.id(), s2.id(), s3.id()]);

    // Completely remove a subscriber, both global and per-topic subscriptions.
    assert!(subs.remove_subscriber(s1.id()));
    assert_subscribers_eq!(subs.get_subscribers(chid(1)), [s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(2)), [s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(3)), [s2.id(), s3.id()]);
    assert_subscribers_eq!(subs.get_subscribers(chid(99)), [s3.id()]);
}
