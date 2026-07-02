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
            events.push(parse_block(&block));
        }
        events
    }

    pub fn finish(&mut self) -> Vec<Result<SseEvent, SseError>> {
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            return Vec::new();
        }
        let block = std::mem::take(&mut self.buffer);
        vec![parse_block(&block)]
    }
}

fn parse_block(block: &str) -> Result<SseEvent, SseError> {
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
        return Err(SseError::MissingData);
    }
    Ok(SseEvent {
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
