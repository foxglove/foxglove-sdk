use std::collections::HashSet;

use crate::channel::ChannelId;

use super::SubscriptionManager;

macro_rules! assert_subscribers {
    ($left:expr, $right:expr) => {
        assert_subscribers!($left, $right,);
    };
    ($left:expr, $right:expr, $($arg:tt),*) => {
        assert_eq!(
            $left.into_iter().collect::<HashSet<_>>(),
            $right.into_iter().collect::<HashSet<_>>(),
            $($arg),*
        );
    };
}

fn chid(id: u64) -> ChannelId {
    ChannelId::new(id)
}

#[test]
fn test_subscriptions() {
    let subs = SubscriptionManager::default();
    assert_subscribers!(subs.get_subscribers(chid(99)), []);

    // Per-topic subscriptions.
    subs.subscribe_channels(1, "s1", [chid(1), chid(2)]);
    subs.subscribe_channels(2, "s2", [chid(2), chid(3)]);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s1"]);
    assert_subscribers!(subs.get_subscribers(chid(2)), ["s1", "s2"]);
    assert_subscribers!(subs.get_subscribers(chid(3)), ["s2"]);
    assert_subscribers!(subs.get_subscribers(chid(99)), []);

    // Global subscription.
    subs.subscribe_global(3, "s3");
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s1", "s3"]);
    assert_subscribers!(subs.get_subscribers(chid(2)), ["s1", "s2", "s3"]);
    assert_subscribers!(subs.get_subscribers(chid(3)), ["s2", "s3"]);
    assert_subscribers!(subs.get_subscribers(chid(99)), ["s3"]);

    // Add a per-topic subscription for an existing global subscriber. The subscriber only appears
    // once in the set of subscribers for the topic.
    subs.subscribe_channels(3, "s3", [chid(3)]);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s1", "s3"]);

    // Removing a topic subscription for a global subscriber doesn't remove the global
    // subscription.
    subs.unsubscribe_channels(&3, [chid(3)]);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s1", "s3"]);

    // Unsubscribe from a particular topic.
    subs.unsubscribe_channels(&1, [chid(1)]);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s3"]);
    assert_subscribers!(subs.get_subscribers(chid(2)), ["s1", "s2", "s3"]);

    // Unsubscribe from multiple topics. Unsubscribe is idempotent.
    subs.unsubscribe_channels(&1, [chid(1), chid(2)]);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s3"]);
    assert_subscribers!(subs.get_subscribers(chid(2)), ["s2", "s3"]);

    // Add a global subscription after a per-topic subscription.
    subs.subscribe_channels(1, "s1", [chid(1)]);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s1", "s3"]);
    subs.subscribe_global(1, "s1");
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s1", "s3"]);
    assert_subscribers!(subs.get_subscribers(chid(2)), ["s1", "s2", "s3"]);
    assert_subscribers!(subs.get_subscribers(chid(99)), ["s1", "s3"]);
    subs.unsubscribe_channels(&1, [chid(1)]);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s1", "s3"]);

    // Completely remove a subscriber, both global and per-topic subscriptions.
    subs.remove_subscriber(&1);
    assert_subscribers!(subs.get_subscribers(chid(1)), ["s3"]);
    assert_subscribers!(subs.get_subscribers(chid(2)), ["s2", "s3"]);
    assert_subscribers!(subs.get_subscribers(chid(99)), ["s3"]);
}
