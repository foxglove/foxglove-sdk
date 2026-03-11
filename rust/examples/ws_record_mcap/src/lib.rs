use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use foxglove::ws_protocol::ParseError;
use foxglove::ws_protocol::client::{Subscribe, Subscription};
use foxglove::ws_protocol::server::ServerMessage;
use foxglove::{
    ChannelBuilder, Context, PartialMetadata, RawChannel, Schema, WebSocketClient,
    WebSocketClientError,
};
use tracing::info;

/// Decodes the schema from an advertised channel, if present.
pub fn decode_schema(adv_ch: &foxglove::ws_protocol::server::Channel<'_>) -> Option<Schema> {
    let schema_encoding = adv_ch.schema_encoding.as_deref()?;
    if schema_encoding.is_empty() {
        return None;
    }
    let schema_data = adv_ch.decode_schema().ok()?;
    Some(Schema::new(
        adv_ch.schema_name.as_ref().to_owned(),
        schema_encoding.to_owned(),
        schema_data,
    ))
}

/// Runs the subscribe-and-record loop until `shutdown` resolves or the server closes the
/// connection.
///
/// Assumes `client` has already performed the initial ServerInfo handshake. Messages are written
/// to the channels registered on `ctx`. Returns the number of messages written.
pub async fn record_stream(
    client: &mut WebSocketClient,
    ctx: &Arc<Context>,
    topic_filter: &[String],
    mut shutdown: impl std::future::Future<Output = ()> + std::marker::Unpin,
) -> Result<u64> {
    let mut server_channels: HashMap<u64, (u32, Arc<RawChannel>)> = HashMap::new();
    let mut subscriptions: HashMap<u32, Arc<RawChannel>> = HashMap::new();
    let mut next_sub_id: u32 = 0;
    let mut msg_count: u64 = 0;

    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
    ticker.tick().await; // consume the immediate first tick

    loop {
        let mut pending_subs: Vec<Subscription> = Vec::new();

        tokio::select! {
            biased;

            _ = &mut shutdown => {
                info!("Shutting down...");
                break;
            }

            _ = ticker.tick() => {
                info!("Messages written: {msg_count}");
                continue;
            }

            result = client.recv() => {
                let msg = match result {
                    Ok(msg) => msg,
                    // recv() has a 1-second internal timeout; just retry.
                    Err(WebSocketClientError::Timeout(_)) => continue,
                    Err(WebSocketClientError::UnexpectedEndOfStream) => {
                        info!("Server closed the connection");
                        break;
                    }
                    Err(WebSocketClientError::ParseError(
                        ParseError::UnhandledMessageType,
                    )) => {
                        tracing::warn!("Received unhandled message type; treating as end of stream");
                        break;
                    }
                    Err(e) => return Err(e.into()),
                };

                match msg {
                    ServerMessage::Advertise(advertise) => {
                        for adv_ch in advertise.channels {
                            let topic = adv_ch.topic.to_string();

                            // Skip topics that don't match the filter (if one was provided).
                            if !topic_filter.is_empty()
                                && !topic_filter.iter().any(|t| *t == topic)
                            {
                                continue;
                            }

                            // Skip channels we're already subscribed to.
                            if server_channels.contains_key(&adv_ch.id) {
                                continue;
                            }

                            let schema = decode_schema(&adv_ch);
                            let encoding = adv_ch.encoding.to_string();
                            let ch_id = adv_ch.id;

                            let channel = ChannelBuilder::new(topic.as_str())
                                .message_encoding(encoding)
                                .schema(schema)
                                .context(ctx)
                                .build_raw()
                                .with_context(|| {
                                    format!("Failed to create channel for {topic}")
                                })?;

                            let sub_id = next_sub_id;
                            next_sub_id += 1;

                            info!("Subscribing to {topic} (sub_id={sub_id})");
                            pending_subs.push(Subscription::new(sub_id, ch_id));
                            subscriptions.insert(sub_id, channel.clone());
                            server_channels.insert(ch_id, (sub_id, channel));
                        }
                    }

                    ServerMessage::Unadvertise(unadvertise) => {
                        for ch_id in unadvertise.channel_ids {
                            if let Some((sub_id, _)) = server_channels.remove(&ch_id) {
                                subscriptions.remove(&sub_id);
                                info!("Channel {ch_id} unadvertised");
                            }
                        }
                    }

                    ServerMessage::MessageData(msg) => {
                        if let Some(channel) = subscriptions.get(&msg.subscription_id) {
                            channel.log_with_meta(
                                &msg.data,
                                PartialMetadata {
                                    log_time: Some(msg.log_time),
                                },
                            );
                            msg_count += 1;
                        }
                    }

                    _ => {}
                }
            }
        }

        // Send any new subscriptions accumulated during this iteration. This is done outside the
        // select! block so that `client` is not borrowed by the received message at the same time.
        if !pending_subs.is_empty() {
            client.send(&Subscribe::new(pending_subs)).await?;
        }
    }

    Ok(msg_count)
}
