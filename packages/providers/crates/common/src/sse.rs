//! Minimal Server-Sent Events decoder.

/// One decoded Server-Sent Event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SseError {
    #[error("SSE event had no data lines")]
    MissingData,
}

/// Incrementally decodes SSE records split by blank lines.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_str(&mut self, chunk: &str) -> Vec<Result<SseEvent, SseError>> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();
        while let Some(boundary) = find_event_boundary(&self.buffer) {
            let block = self.buffer[..boundary].to_owned();
            let drain_to = boundary + boundary_len(&self.buffer[boundary..]);
            self.buffer.drain(..drain_to);
            if block.trim().is_empty() {
                continue;
            }
            if let Some(event) = parse_block(&block) {
                events.push(Ok(event));
            }
        }
        events
    }

    pub fn finish(&mut self) -> Vec<Result<SseEvent, SseError>> {
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            return Vec::new();
        }
        let block = std::mem::take(&mut self.buffer);
        parse_block(&block).map(Ok).into_iter().collect()
    }
}

/// A block with no `data:` line — a comment-only heartbeat (`: ping`) or an
/// `event:`-only frame — is valid SSE, not a protocol violation: per the
/// spec, an event with an empty data buffer is simply never dispatched.
/// Servers and intermediary proxies rely on exactly this to keep long-lived
/// connections alive during slow model turns, so treating it as fatal turns
/// routine keep-alives into spurious stream failures.
fn parse_block(block: &str) -> Option<SseEvent> {
    let mut event = None;
    let mut data = Vec::new();
    for line in block.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_owned());
        }
    }
    if data.is_empty() {
        return None;
    }
    Some(SseEvent {
        event,
        data: data.join("\n"),
    })
}

fn find_event_boundary(buffer: &str) -> Option<usize> {
    ["\r\n\r\n", "\n\n", "\r\r"]
        .iter()
        .filter_map(|needle| buffer.find(needle))
        .min()
}

fn boundary_len(rest: &str) -> usize {
    if rest.starts_with("\r\n\r\n") { 4 } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_split_events() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push_str("event: message\ndata: {\"a\"").is_empty());
        let events = decoder.push_str(":1}\n\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(event) => {
                assert_eq!(event.event.as_deref(), Some("message"));
                assert_eq!(event.data, "{\"a\":1}");
            }
            Err(err) => panic!("unexpected SSE parse error: {err}"),
        }
    }

    #[test]
    fn comment_only_heartbeat_is_ignored_not_an_error() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push_str(": keep-alive\n\n");
        assert!(
            events.is_empty(),
            "a bare comment heartbeat must not surface as an event or an error, got {events:?}"
        );
    }

    #[test]
    fn event_only_block_with_no_data_is_ignored_not_an_error() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push_str("event: ping\n\n");
        assert!(
            events.is_empty(),
            "an event field with no data line must not be dispatched or errored, got {events:?}"
        );
    }

    #[test]
    fn heartbeat_between_real_events_does_not_disrupt_decoding() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push_str("data: first\n\n: keep-alive\n\ndata: second\n\n");
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], Ok(event) if event.data == "first"));
        assert!(matches!(&events[1], Ok(event) if event.data == "second"));
    }

    #[test]
    fn finish_on_trailing_heartbeat_yields_no_events() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push_str(": keep-alive").is_empty());
        assert!(decoder.finish().is_empty());
    }

    #[test]
    fn joins_multiple_data_lines() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push_str("data: one\ndata: two\r\n\r\n");
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(event) => assert_eq!(event.data, "one\ntwo"),
            Err(err) => panic!("unexpected SSE parse error: {err}"),
        }
    }
}
