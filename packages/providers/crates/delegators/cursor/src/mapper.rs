use agentloop_contracts::{ToolCallId, ToolOutput, ToolResultBlock, TurnStopReason};
use agentloop_delegator_common::{DelegatorEvent, DelegatorMapError, LineMapper};
use serde::Deserialize;

/// Maps `cursor-agent --print --output-format stream-json` lines into
/// normalized events. The wire format is modeled on Claude Code's stream-json
/// (`system`/`user`/`assistant`/`result` envelopes); the `system` and `user`
/// frames in the tests were recorded live (2026-07-08), the rest follow the
/// same documented shape.
#[derive(Debug, Default)]
pub struct CursorLineMapper {
    /// cursor-agent echoes the final text again in its `result` frame; once
    /// assistant text has streamed the echo must be suppressed.
    saw_assistant_text: bool,
}

impl CursorLineMapper {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LineMapper for CursorLineMapper {
    fn map_line(&mut self, line: &str) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let event: CursorWireEvent = serde_json::from_str(trimmed)?;
        Ok(match event {
            // Session bookkeeping / prompt echo — nothing to surface.
            CursorWireEvent::System | CursorWireEvent::User => Vec::new(),
            CursorWireEvent::Assistant { message } => {
                let events = map_assistant_message(message)?;
                if events
                    .iter()
                    .any(|event| matches!(event, DelegatorEvent::AssistantDelta { .. }))
                {
                    self.saw_assistant_text = true;
                }
                events
            }
            CursorWireEvent::ToolCall { id, name, input } => vec![DelegatorEvent::ToolCall {
                call_id: id
                    .map(ToolCallId::from)
                    .unwrap_or_else(ToolCallId::generate),
                name,
                args: input,
            }],
            CursorWireEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => vec![DelegatorEvent::ToolResult {
                call_id: tool_use_id
                    .map(ToolCallId::from)
                    .ok_or(DelegatorMapError::MissingField("tool_use_id"))?,
                output: tool_output_from_value(
                    content.unwrap_or(serde_json::Value::Null),
                    is_error,
                ),
            }],
            CursorWireEvent::Result { result, is_error } => {
                let mut events = Vec::new();
                if !self.saw_assistant_text {
                    if let Some(result) = result.filter(|value| !value.is_empty()) {
                        events.push(DelegatorEvent::AssistantDelta { text: result });
                    }
                }
                events.push(DelegatorEvent::TurnFinished {
                    stop_reason: if is_error.unwrap_or(false) {
                        TurnStopReason::Error
                    } else {
                        TurnStopReason::EndTurn
                    },
                });
                self.saw_assistant_text = false;
                events
            }
            CursorWireEvent::Error { message } => vec![DelegatorEvent::Error { message }],
            CursorWireEvent::Unknown => vec![DelegatorEvent::Unknown {
                kind: "unknown".to_owned(),
            }],
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CursorWireEvent {
    System,
    User,
    Assistant {
        message: CursorAssistantMessage,
    },
    ToolCall {
        id: Option<String>,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: Option<String>,
        #[serde(default)]
        content: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
    },
    Result {
        #[serde(default)]
        result: Option<String>,
        #[serde(default)]
        is_error: Option<bool>,
    },
    Error {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct CursorAssistantMessage {
    #[serde(default)]
    content: Vec<CursorContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CursorContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: Option<String>,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: Option<String>,
        #[serde(default)]
        content: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Unknown,
}

fn map_assistant_message(
    message: CursorAssistantMessage,
) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
    let mut events = Vec::new();
    for block in message.content {
        match block {
            CursorContentBlock::Text { text } => {
                events.push(DelegatorEvent::AssistantDelta { text });
            }
            CursorContentBlock::ToolUse { id, name, input } => {
                events.push(DelegatorEvent::ToolCall {
                    call_id: id
                        .map(ToolCallId::from)
                        .unwrap_or_else(ToolCallId::generate),
                    name,
                    args: input,
                });
            }
            CursorContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                events.push(DelegatorEvent::ToolResult {
                    call_id: tool_use_id
                        .map(ToolCallId::from)
                        .ok_or(DelegatorMapError::MissingField("tool_use_id"))?,
                    output: tool_output_from_value(
                        content.unwrap_or(serde_json::Value::Null),
                        is_error,
                    ),
                });
            }
            CursorContentBlock::Unknown => {}
        }
    }
    Ok(events)
}

fn tool_output_from_value(value: serde_json::Value, is_error: bool) -> ToolOutput {
    let text = match &value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        other => match serde_json::to_string(other) {
            Ok(text) => text,
            Err(_) => other.to_string(),
        },
    };
    ToolOutput {
        content: vec![ToolResultBlock::markdown(text)],
        is_error,
        structured: (!value.is_null()).then_some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Recorded live from `cursor-agent --print --output-format stream-json`
    // (cursor-agent on PATH, 2026-07-08). Never hand-edit; re-record instead.
    const LIVE_SYSTEM_INIT: &str = r#"{"type":"system","subtype":"init","apiKeySource":"login","cwd":"/private/tmp/work","session_id":"37618ac0-4d30-4dd4-bb87-1abc269069b4","model":"Composer 2.5 Fast","permissionMode":"default"}"#;
    const LIVE_USER_ECHO: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Read the file note.txt and tell me its contents"}]},"session_id":"37618ac0-4d30-4dd4-bb87-1abc269069b4"}"#;
    // UNVERIFIED: recorded from cursor-agent's documented stream-json format
    // (Claude-Code-shaped), not a live CLI — the live run hit a usage limit
    // before assistant output. Re-record when a working account is available.
    const DOC_ASSISTANT: &str = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"pong"}]},"session_id":"37618ac0"}"#;
    const DOC_RESULT: &str = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":1204,"result":"pong","session_id":"37618ac0"}"#;

    #[test]
    fn system_and_user_frames_map_to_nothing() {
        let mut mapper = CursorLineMapper::new();
        for line in [LIVE_SYSTEM_INIT, LIVE_USER_ECHO] {
            match mapper.map_line(line) {
                Ok(events) => assert!(events.is_empty(), "expected no events for {line}"),
                Err(err) => panic!("recorded line should parse: {err}"),
            }
        }
    }

    #[test]
    fn assistant_text_streams_and_result_echo_is_deduped() {
        let mut mapper = CursorLineMapper::new();
        let events = match mapper.map_line(DOC_ASSISTANT) {
            Ok(events) => events,
            Err(err) => panic!("assistant line should parse: {err}"),
        };
        assert_eq!(
            events,
            vec![DelegatorEvent::AssistantDelta {
                text: "pong".to_owned()
            }]
        );
        let events = match mapper.map_line(DOC_RESULT) {
            Ok(events) => events,
            Err(err) => panic!("result line should parse: {err}"),
        };
        assert_eq!(
            events,
            vec![DelegatorEvent::TurnFinished {
                stop_reason: TurnStopReason::EndTurn
            }]
        );
    }

    #[test]
    fn result_text_is_used_when_nothing_streamed() {
        let mut mapper = CursorLineMapper::new();
        let events = match mapper.map_line(DOC_RESULT) {
            Ok(events) => events,
            Err(err) => panic!("result line should parse: {err}"),
        };
        assert_eq!(
            events,
            vec![
                DelegatorEvent::AssistantDelta {
                    text: "pong".to_owned()
                },
                DelegatorEvent::TurnFinished {
                    stop_reason: TurnStopReason::EndTurn
                },
            ]
        );
    }

    #[test]
    fn unknown_frames_are_routed_to_unknown() {
        let mut mapper = CursorLineMapper::new();
        match mapper.map_line(r#"{"type":"totally_new_frame"}"#) {
            Ok(events) => assert!(matches!(
                events.first(),
                Some(DelegatorEvent::Unknown { .. })
            )),
            Err(err) => panic!("unknown frames must not error: {err}"),
        }
    }
}
