use std::net::SocketAddr;

use flume::TrySendError;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Copy)]
pub(crate) enum SendLossyResult {
    Sent,
    #[allow(dead_code)]
    SentLossy(usize),
    ExhaustedRetries,
}

/// Attempt to send a message on the channel.
///
/// If the channel is non-full, this function returns `SendLossyResult::Sent`.
///
/// If the channel is full, drop the oldest message and try again. If the send eventually succeeds
/// in this manner, this function returns `SendLossyResult::SentLossy(dropped)`. If the maximum
/// number of retries is reached, it returns `SendLossyResult::ExhaustedRetries`.
pub(crate) fn send_lossy(
    client_addr: &SocketAddr,
    tx: &flume::Sender<Message>,
    rx: &flume::Receiver<Message>,
    mut message: Message,
    retries: usize,
) -> SendLossyResult {
    // If the queue is full, drop the oldest message(s). We do this because the websocket
    // client is falling behind, and we either start dropping messages, or we'll end up
    // buffering until we run out of memory. There's no point in that because the client is
    // unlikely to catch up and be able to consume the messages.
    let mut dropped = 0;
    loop {
        match (dropped, tx.try_send(message)) {
            (0, Ok(_)) => return SendLossyResult::Sent,
            (dropped, Ok(_)) => {
                tracing::warn!(
                    "outbox for client {} full, dropped {dropped} messages",
                    client_addr
                );
                return SendLossyResult::SentLossy(dropped);
            }
            (_, Err(TrySendError::Disconnected(_))) => unreachable!("we're holding rx"),
            (_, Err(TrySendError::Full(rejected))) => {
                if dropped >= retries {
                    tracing::warn!(
                        "outbox for client {} full, dropping message after 10 attempts",
                        client_addr
                    );
                    return SendLossyResult::ExhaustedRetries;
                }
                message = rejected;
                let _ = rx.try_recv();
                dropped += 1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use tracing_test::traced_test;

    use super::*;

    fn make_message(id: usize) -> Message {
        Message::Text(format!("{id}").into())
    }

    fn parse_message(msg: Message) -> usize {
        match msg {
            Message::Text(text) => text.parse().expect("id"),
            _ => unreachable!(),
        }
    }

    #[traced_test]
    #[test]
    fn test_send_lossy() {
        const BACKLOG: usize = 4;
        const TOTAL: usize = 10;

        let addr = SocketAddr::new("127.0.0.1".parse().unwrap(), 1234);

        let (tx, rx) = flume::bounded(BACKLOG);
        for i in 0..BACKLOG {
            assert_matches!(
                send_lossy(&addr, &tx, &rx, make_message(i), 0),
                SendLossyResult::Sent
            );
        }

        // The queue is full now. We'll only succeed with retries.
        for i in BACKLOG..TOTAL {
            assert_matches!(
                send_lossy(&addr, &tx, &rx, make_message(TOTAL + i), 0),
                SendLossyResult::ExhaustedRetries
            );
            assert_matches!(
                send_lossy(&addr, &tx, &rx, make_message(i), 1),
                SendLossyResult::SentLossy(1)
            );
        }

        // Receive everything, expect that the first (TOTAL - BACKLOG) messages were dropped.
        let mut received: Vec<usize> = vec![];
        while let Ok(msg) = rx.try_recv() {
            received.push(parse_message(msg));
        }
        assert_eq!(received, ((TOTAL - BACKLOG)..TOTAL).collect::<Vec<_>>());
    }
}
