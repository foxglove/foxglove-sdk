use std::sync::Arc;

use crate::{testutil::RecordingSink, ChannelBuilder, Sink};

use super::*;

#[test]
fn test_subscription_aggregator() {
    let ctx = Context::new();

    let mut channels = vec![];
    for topic in 0..3 {
        let ch = ChannelBuilder::new(format!("/ch{topic}"))
            .context(&ctx)
            .message_encoding("")
            .build()
            .unwrap();
        channels.push(ch.id());
    }

    let sink = Arc::new(RecordingSink::new().auto_subscribe(false));
    assert!(ctx.add_sink(sink.clone()));
    assert!(channels.iter().all(|&id| !ctx.has_subscribers(id)));

    // Create a subscription aggregator and some fake client IDs.
    let sa = SubscriptionAggregator::new(Arc::downgrade(&ctx), sink.id());
    let clients: Vec<_> = (1..3).map(ClientId::new).collect();

    // Subscribe to some channels.
    sa.subscribe_channels(clients[0], [channels[0], channels[1]]);
    assert!(ctx.has_subscribers(channels[0]));
    assert!(ctx.has_subscribers(channels[1]));
    assert!(!ctx.has_subscribers(channels[2]));

    // Subscribe to some overlapping channels with a different client.
    sa.subscribe_channels(clients[1], [channels[1], channels[2]]);
    assert!(channels.iter().all(|&id| ctx.has_subscribers(id)));

    // Unsubscribe first client from ch0.
    sa.unsubscribe_channels(clients[0], [channels[0]]);
    assert!(!ctx.has_subscribers(channels[0]));
    assert!(ctx.has_subscribers(channels[1]));
    assert!(ctx.has_subscribers(channels[2]));

    // Unsubscribe first client from ch1. No effect, since second client is still subscribed.
    sa.unsubscribe_channels(clients[0], [channels[1]]);
    assert!(!ctx.has_subscribers(channels[0]));
    assert!(ctx.has_subscribers(channels[1]));
    assert!(ctx.has_subscribers(channels[2]));

    // Idempotence
    sa.unsubscribe_channels(clients[0], [channels[1]]);
    assert!(!ctx.has_subscribers(channels[0]));
    assert!(ctx.has_subscribers(channels[1]));
    assert!(ctx.has_subscribers(channels[2]));
    sa.subscribe_channels(clients[1], [channels[1]]);
    assert!(!ctx.has_subscribers(channels[0]));
    assert!(ctx.has_subscribers(channels[1]));
    assert!(ctx.has_subscribers(channels[2]));

    // Remove second client's subscriptions.
    sa.unsubscribe_channels(clients[1], [channels[1], channels[2]]);
    assert!(channels.iter().all(|&id| !ctx.has_subscribers(id)));
}
