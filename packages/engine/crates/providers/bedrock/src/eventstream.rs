//! Minimal decoder for the AWS event-stream (`vnd.amazon.eventstream`) framing
//! Bedrock uses for streaming responses.
//!
//! Each message is:
//! ```text
//! [total len: u32][headers len: u32][prelude CRC: u32]
//! [headers ...][payload ...][message CRC: u32]
//! ```
//! Headers are `[name len: u8][name][value type: u8][value ...]`. We only need
//! the string-typed `:event-type` / `:message-type` / `:exception-type` headers
//! to route the payload; other header value types are skipped by their size.
//! CRCs are not verified (length framing + JSON parsing already catch garbage).

/// The routing headers plus raw payload of one decoded frame.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RawEvent {
    pub event_type: Option<String>,
    pub message_type: Option<String>,
    pub exception_type: Option<String>,
    pub payload: Vec<u8>,
}

/// Accumulates bytes and yields whole frames as they complete.
#[derive(Debug, Default)]
pub(crate) struct EventStreamDecoder {
    buf: Vec<u8>,
}

impl EventStreamDecoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Pull the next complete frame. `Ok(None)` means more bytes are needed;
    /// `Err` means the framing is corrupt (unrecoverable for this stream).
    pub(crate) fn next_message(&mut self) -> Result<Option<RawEvent>, String> {
        // Prelude is 12 bytes: total len, headers len, prelude CRC.
        if self.buf.len() < 12 {
            return Ok(None);
        }
        let total_len = be_u32(&self.buf[0..4]) as usize;
        let headers_len = be_u32(&self.buf[4..8]) as usize;
        // 16 = 4 prelude lengths (8) + prelude CRC (4) + message CRC (4).
        if total_len < 16 || headers_len > total_len - 16 {
            return Err(format!(
                "invalid event-stream frame: total={total_len}, headers={headers_len}"
            ));
        }
        if self.buf.len() < total_len {
            return Ok(None);
        }
        let headers_start = 12;
        let headers_end = headers_start + headers_len;
        let payload_end = total_len - 4;
        let event = parse_headers(&self.buf[headers_start..headers_end]);
        let payload = self.buf[headers_end..payload_end].to_vec();
        self.buf.drain(..total_len);
        Ok(Some(RawEvent { payload, ..event }))
    }
}

fn be_u32(bytes: &[u8]) -> u32 {
    // Callers pass a 4-byte window guaranteed by a prior length check.
    let mut value = 0u32;
    for &byte in bytes.iter().take(4) {
        value = (value << 8) | u32::from(byte);
    }
    value
}

fn be_u16(bytes: &[u8]) -> usize {
    (usize::from(bytes[0]) << 8) | usize::from(bytes[1])
}

/// Extract the routing headers we care about; skip everything else by size.
fn parse_headers(mut headers: &[u8]) -> RawEvent {
    let mut event = RawEvent::default();
    while !headers.is_empty() {
        let name_len = usize::from(headers[0]);
        headers = &headers[1..];
        if headers.len() < name_len {
            break;
        }
        let name = String::from_utf8_lossy(&headers[..name_len]).into_owned();
        headers = &headers[name_len..];
        if headers.is_empty() {
            break;
        }
        let value_type = headers[0];
        headers = &headers[1..];
        match value_type {
            // string (7) and byte-array (6): u16 length prefix + bytes.
            6 | 7 => {
                if headers.len() < 2 {
                    break;
                }
                let value_len = be_u16(&headers[..2]);
                headers = &headers[2..];
                if headers.len() < value_len {
                    break;
                }
                let value = String::from_utf8_lossy(&headers[..value_len]).into_owned();
                headers = &headers[value_len..];
                assign(&mut event, &name, value);
            }
            0 | 1 => {}                          // bool true/false
            2 => headers = skip(headers, 1),     // byte
            3 => headers = skip(headers, 2),     // short
            4 => headers = skip(headers, 4),     // int
            5 | 8 => headers = skip(headers, 8), // long / timestamp
            9 => headers = skip(headers, 16),    // uuid
            _ => break,                          // unknown: stop safely
        }
    }
    event
}

fn skip(bytes: &[u8], n: usize) -> &[u8] {
    if bytes.len() < n { &[] } else { &bytes[n..] }
}

fn assign(event: &mut RawEvent, name: &str, value: String) {
    match name {
        ":event-type" => event.event_type = Some(value),
        ":message-type" => event.message_type = Some(value),
        ":exception-type" | ":error-code" => event.exception_type = Some(value),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one event-stream frame with a single `:event-type` string header.
    fn frame(event_type: &str, payload: &[u8]) -> Vec<u8> {
        // header: name_len(1) + name + type(1) + u16 len + value
        let name = b":event-type";
        let mut headers = Vec::new();
        headers.push(name.len() as u8);
        headers.extend_from_slice(name);
        headers.push(7u8); // string
        headers.extend_from_slice(&(event_type.len() as u16).to_be_bytes());
        headers.extend_from_slice(event_type.as_bytes());

        let total = 12 + headers.len() + payload.len() + 4;
        let mut out = Vec::new();
        out.extend_from_slice(&(total as u32).to_be_bytes());
        out.extend_from_slice(&(headers.len() as u32).to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes()); // prelude CRC (unchecked)
        out.extend_from_slice(&headers);
        out.extend_from_slice(payload);
        out.extend_from_slice(&0u32.to_be_bytes()); // message CRC (unchecked)
        out
    }

    #[test]
    fn decodes_a_single_frame() {
        let bytes = frame("contentBlockDelta", br#"{"delta":{"text":"hi"}}"#);
        let mut decoder = EventStreamDecoder::new();
        decoder.push(&bytes);
        let event = decoder.next_message().expect("ok").expect("frame");
        assert_eq!(event.event_type.as_deref(), Some("contentBlockDelta"));
        assert_eq!(event.payload, br#"{"delta":{"text":"hi"}}"#);
        assert_eq!(decoder.next_message().expect("ok"), None);
    }

    #[test]
    fn waits_for_more_bytes_when_frame_is_split() {
        let bytes = frame("messageStop", br#"{"stopReason":"end_turn"}"#);
        let (head, tail) = bytes.split_at(10);
        let mut decoder = EventStreamDecoder::new();
        decoder.push(head);
        assert_eq!(decoder.next_message().expect("ok"), None); // incomplete
        decoder.push(tail);
        let event = decoder.next_message().expect("ok").expect("frame");
        assert_eq!(event.event_type.as_deref(), Some("messageStop"));
    }

    #[test]
    fn decodes_two_concatenated_frames() {
        let mut bytes = frame("messageStart", br#"{"role":"assistant"}"#);
        bytes.extend(frame("contentBlockDelta", br#"{"delta":{"text":"a"}}"#));
        let mut decoder = EventStreamDecoder::new();
        decoder.push(&bytes);
        assert_eq!(
            decoder
                .next_message()
                .expect("ok")
                .expect("f1")
                .event_type
                .as_deref(),
            Some("messageStart")
        );
        assert_eq!(
            decoder
                .next_message()
                .expect("ok")
                .expect("f2")
                .event_type
                .as_deref(),
            Some("contentBlockDelta")
        );
        assert_eq!(decoder.next_message().expect("ok"), None);
    }

    #[test]
    fn rejects_impossible_lengths() {
        let mut decoder = EventStreamDecoder::new();
        // total len = 4 (< 16 minimum) but 12+ bytes buffered.
        decoder.push(&[0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(decoder.next_message().is_err());
    }
}
