//! Minimal Server-Sent Events parser for remote access watch streams.

use std::{collections::VecDeque, pin::Pin};

use bytes::Bytes;
use futures_util::{Stream, StreamExt};

/// A single event parsed from the SSE stream.
#[derive(Debug, Default, Clone)]
pub(super) struct SseEvent {
    pub(super) event: String,
    pub(super) data: String,
}

/// One frame yielded by the SSE parser. Comments are surfaced so callers can observe
/// wire-heartbeats and reset their read-timeouts.
#[derive(Debug, Clone)]
pub(super) enum SseFrame {
    Comment,
    Event(SseEvent),
}

/// Wraps a byte stream in an SSE parser that yields one [`SseFrame`] per `\n\n`-delimited
/// event or per `:`-prefixed comment line. Comments are surfaced rather than silently consumed
/// so the consumer can use them as keep-alive signals.
pub(super) fn sse_event_stream<S>(
    bytes: S,
) -> Pin<Box<dyn Stream<Item = Result<SseFrame, reqwest::Error>> + Send>>
where
    S: Stream<Item = reqwest::Result<Bytes>> + Send + 'static,
{
    Box::pin(futures_util::stream::unfold(
        (Box::pin(bytes), SseParser::default(), false),
        |(mut bytes, mut parser, mut finished)| async move {
            loop {
                if let Some(frame) = parser.pop() {
                    return Some((Ok(frame), (bytes, parser, finished)));
                }
                if finished {
                    return None;
                }
                match bytes.next().await {
                    Some(Ok(chunk)) => parser.feed(&chunk),
                    Some(Err(e)) => return Some((Err(e), (bytes, parser, finished))),
                    None => {
                        finished = true;
                        if let Some(frame) = parser.flush() {
                            return Some((Ok(frame), (bytes, parser, finished)));
                        }
                    }
                }
            }
        },
    ))
}

/// Stateful accumulator for SSE-framed bytes. Handles `\n`, `\r\n`, and `\r` line terminators,
/// but is deliberately minimal: it recognizes only `event:` and `data:` fields, surfaces
/// comments as [`SseFrame::Comment`] keep-alive markers, and drops malformed UTF-8 lines.
#[derive(Default)]
struct SseParser {
    buffer: Vec<u8>,
    pending: SseEvent,
    has_data: bool,
    ready: VecDeque<SseFrame>,
}

impl SseParser {
    fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        while let Some(end) = self.take_line_end() {
            // `end` is an inclusive index into the terminator. Slice before the terminator.
            let (term_start, term_len) = end;
            let line = {
                let slice = &self.buffer[..term_start];
                std::str::from_utf8(slice).map(|s| s.to_string())
            };
            // Drain the line and its terminator from the buffer.
            self.buffer.drain(..term_start + term_len);
            let Ok(line) = line else { continue };
            self.handle_line(&line);
        }
    }

    /// Flush any event sitting in the accumulator. Called once on EOF.
    fn flush(&mut self) -> Option<SseFrame> {
        self.dispatch();
        self.ready.pop_front()
    }

    fn pop(&mut self) -> Option<SseFrame> {
        self.ready.pop_front()
    }

    /// Find the start of the next line terminator in `self.buffer`.
    /// Returns `(term_start, term_len)` where `term_len` is 1 for `\n`/`\r` alone and 2 for `\r\n`.
    fn take_line_end(&self) -> Option<(usize, usize)> {
        for (i, &b) in self.buffer.iter().enumerate() {
            match b {
                b'\n' => return Some((i, 1)),
                b'\r' => {
                    let next_is_lf = self.buffer.get(i + 1) == Some(&b'\n');
                    if next_is_lf {
                        return Some((i, 2));
                    }
                    // Bare `\r` is only a terminator if we've seen the next byte already; if we
                    // don't yet have byte i+1, wait for more data.
                    if i + 1 < self.buffer.len() {
                        return Some((i, 1));
                    }
                    return None;
                }
                _ => {}
            }
        }
        None
    }

    fn handle_line(&mut self, line: &str) {
        if line.is_empty() {
            self.dispatch();
            return;
        }
        if line.starts_with(':') {
            // Surface comments (e.g. wire-heartbeat keep-alives) as a frame so the consumer
            // can reset its read-timeout. The contents of the comment are discarded.
            self.ready.push_back(SseFrame::Comment);
            return;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => {
                self.pending.event = value.to_string();
            }
            "data" => {
                if self.has_data {
                    self.pending.data.push('\n');
                }
                self.pending.data.push_str(value);
                self.has_data = true;
            }
            _ => {} // ignore id/retry and any unknown field
        }
    }

    fn dispatch(&mut self) {
        // Per the SSE spec, dispatch only if we have observed at least one data line in the
        // current frame. This also ensures consecutive blank lines don't emit empty events.
        if !self.has_data {
            self.pending = SseEvent::default();
            return;
        }
        let event = std::mem::take(&mut self.pending);
        self.has_data = false;
        self.ready.push_back(SseFrame::Event(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(input: &[u8]) -> Vec<SseFrame> {
        let mut parser = SseParser::default();
        parser.feed(input);
        let mut out = Vec::new();
        while let Some(ev) = parser.pop() {
            out.push(ev);
        }
        if let Some(ev) = parser.flush() {
            out.push(ev);
        }
        out
    }

    fn only_events(frames: Vec<SseFrame>) -> Vec<SseEvent> {
        frames
            .into_iter()
            .filter_map(|f| match f {
                SseFrame::Event(ev) => Some(ev),
                SseFrame::Comment => None,
            })
            .collect()
    }

    fn count_comments(frames: &[SseFrame]) -> usize {
        frames
            .iter()
            .filter(|f| matches!(f, SseFrame::Comment))
            .count()
    }

    #[test]
    fn parse_basic_event() {
        let events = only_events(parse_all(b"event: hello\ndata: {\"a\":1}\n\n"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "hello");
        assert_eq!(events[0].data, "{\"a\":1}");
    }

    #[test]
    fn parse_two_events() {
        let events = only_events(parse_all(
            b"event: hello\ndata: {\"watchLeaseId\":\"rwl_1\"}\n\nevent: wake\ndata: {\"token\":\"abc\"}\n\n",
        ));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "hello");
        assert_eq!(events[1].event, "wake");
        assert_eq!(events[1].data, "{\"token\":\"abc\"}");
    }

    #[test]
    fn comments_are_yielded_as_keepalives() {
        let frames = parse_all(b": keepalive\n: another\nevent: wake\ndata: {}\n\n");
        assert_eq!(count_comments(&frames), 2);
        let events = only_events(frames);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "wake");
    }

    #[test]
    fn handles_crlf_terminators() {
        let events = only_events(parse_all(b"event: hello\r\ndata: {}\r\n\r\n"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "hello");
        assert_eq!(events[0].data, "{}");
    }

    #[test]
    fn multiline_data_concatenates_with_newline() {
        let events = only_events(parse_all(b"event: hello\ndata: line1\ndata: line2\n\n"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[test]
    fn chunked_feeds_produce_single_event() {
        let mut parser = SseParser::default();
        parser.feed(b"event: hel");
        parser.feed(b"lo\ndata: {\"a");
        parser.feed(b"\":1}\n\n");
        let events: Vec<_> = only_events(std::iter::from_fn(|| parser.pop()).collect());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "hello");
        assert_eq!(events[0].data, "{\"a\":1}");
    }

    #[test]
    fn blank_lines_without_data_do_not_emit() {
        let frames = parse_all(b"\n\n\n");
        assert!(frames.is_empty());
    }

    #[test]
    fn event_without_data_is_dropped() {
        let frames = parse_all(b"event: wake\n\n");
        assert!(frames.is_empty());
    }

    #[test]
    fn field_without_space_is_parsed() {
        // The space after the colon is optional per the spec.
        let events = only_events(parse_all(b"event:wake\ndata:{}\n\n"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "wake");
        assert_eq!(events[0].data, "{}");
    }

    #[test]
    fn comment_only_input_yields_comment_frame() {
        let frames = parse_all(b": keepalive\n");
        assert_eq!(count_comments(&frames), 1);
        assert!(only_events(frames).is_empty());
    }
}
