use agentloop_contracts::{ToolCallId, ToolOutput, ToolResultBlock, TurnStopReason};
use agentloop_delegator_common::{DelegatorEvent, DelegatorMapError, LineMapper};
use serde::Deserialize;

#[derive(Debug, Default)]
pub struct ClaudeCodeLineMapper;

impl ClaudeCodeLineMapper {
    pub fn new() -> Self {
        Self
    }
}

impl LineMapper for ClaudeCodeLineMapper {
    fn map_line(&mut self, line: &str) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let event: ClaudeCodeWireEvent = serde_json::from_str(trimmed)?;
        Ok(match event {
            ClaudeCodeWireEvent::AssistantDelta { text } => {
                vec![DelegatorEvent::AssistantDelta { text }]
            }
            ClaudeCodeWireEvent::ToolCall { id, name, input } => {
                vec![DelegatorEvent::ToolCall {
                    call_id: id
                        .map(ToolCallId::from)
                        .unwrap_or_else(ToolCallId::generate),
                    name,
                    args: input,
                }]
            }
            ClaudeCodeWireEvent::ToolResult {
                id,
                tool_use_id,
                content,
                is_error,
            } => vec![DelegatorEvent::ToolResult {
                call_id: tool_use_id
                    .or(id)
                    .map(ToolCallId::from)
                    .ok_or(DelegatorMapError::MissingField("tool_use_id"))?,
                output: tool_output_from_value(
                    content.unwrap_or(serde_json::Value::Null),
                    is_error,
                ),
            }],
            ClaudeCodeWireEvent::Assistant { message } => map_assistant_message(message)?,
            ClaudeCodeWireEvent::Result { result, is_error } => {
                let mut events = Vec::new();
                if let Some(result) = result.filter(|value| !value.is_empty()) {
                    events.push(DelegatorEvent::AssistantDelta { text: result });
                }
                events.push(if is_error.unwrap_or(false) {
                    DelegatorEvent::TurnFinished {
                        stop_reason: TurnStopReason::Error,
                    }
                } else {
                    DelegatorEvent::TurnFinished {
                        stop_reason: TurnStopReason::EndTurn,
                    }
                });
                events
            }
            ClaudeCodeWireEvent::TurnFinished { stop_reason } => {
                vec![DelegatorEvent::TurnFinished {
                    stop_reason: map_stop_reason(stop_reason.as_deref()),
                }]
            }
            ClaudeCodeWireEvent::Error { message } => vec![DelegatorEvent::Error { message }],
            ClaudeCodeWireEvent::Unknown => vec![DelegatorEvent::Unknown {
                kind: "unknown".to_owned(),
            }],
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeCodeWireEvent {
    AssistantDelta {
        text: String,
    },
    ToolCall {
        id: Option<String>,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    ToolResult {
        id: Option<String>,
        tool_use_id: Option<String>,
        #[serde(default)]
        content: Option<serde_json::Value>,
        #[serde(default)]
        is_error: bool,
    },
    Assistant {
        message: ClaudeAssistantMessage,
    },
    Result {
        #[serde(default)]
        result: Option<String>,
        #[serde(default)]
        is_error: Option<bool>,
    },
    TurnFinished {
        stop_reason: Option<String>,
    },
    Error {
        message: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct ClaudeAssistantMessage {
    #[serde(default)]
    content: Vec<ClaudeContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClaudeContentBlock {
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
    message: ClaudeAssistantMessage,
) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
    let mut events = Vec::new();
    for block in message.content {
        match block {
            ClaudeContentBlock::Text { text } => {
                events.push(DelegatorEvent::AssistantDelta { text });
            }
            ClaudeContentBlock::ToolUse { id, name, input } => {
                events.push(DelegatorEvent::ToolCall {
                    call_id: id
                        .map(ToolCallId::from)
                        .unwrap_or_else(ToolCallId::generate),
                    name,
                    args: input,
                });
            }
            ClaudeContentBlock::ToolResult {
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
            ClaudeContentBlock::Unknown => {}
        }
    }
    Ok(events)
}

fn map_stop_reason(reason: Option<&str>) -> TurnStopReason {
    match reason {
        Some("cancelled") => TurnStopReason::Cancelled,
        Some("max_tokens") => TurnStopReason::MaxTokens,
        Some("error") => TurnStopReason::Error,
        _ => TurnStopReason::EndTurn,
    }
}

fn tool_output_from_value(value: serde_json::Value, is_error: bool) -> ToolOutput {
    let text = render_tool_content(&value);
    ToolOutput {
        content: vec![ToolResultBlock::markdown(text)],
        is_error,
        structured: (!value.is_null()).then_some(value),
    }
}

fn render_tool_content(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .map(render_tool_content)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(object) => object
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| compact_json(value)),
        _ => compact_json(value),
    }
}

fn compact_json(value: &serde_json::Value) -> String {
    match serde_json::to_string(value) {
        Ok(text) => text,
        Err(_) => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use agentloop_delegator_common::DelegatorEvent;

    #[test]
    fn maps_fake_assistant_delta() {
        let mut mapper = ClaudeCodeLineMapper::new();
        let events = mapper.map_line(r#"{"type":"assistant_delta","text":"hello"}"#);
        match events {
            Ok(events) => assert_eq!(
                events,
                vec![DelegatorEvent::AssistantDelta {
                    text: "hello".to_owned()
                }]
            ),
            Err(err) => panic!("fake CLI line should parse: {err}"),
        }
    }

    #[test]
    fn maps_fake_tool_call() {
        let mut mapper = ClaudeCodeLineMapper::new();
        let events = mapper.map_line(
            r#"{"type":"tool_call","id":"toolu_1","name":"Read","input":{"file_path":"README.md"}}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(DelegatorEvent::ToolCall { call_id, name, args })
                        if call_id.as_str() == "toolu_1"
                            && name == "Read"
                            && args["file_path"] == "README.md"
                ));
            }
            Err(err) => panic!("fake CLI line should parse: {err}"),
        }
    }

    #[test]
    fn maps_fake_tool_result() {
        let mut mapper = ClaudeCodeLineMapper::new();
        let events = mapper.map_line(
            r#"{"type":"tool_result","tool_use_id":"toolu_1","content":"done","is_error":false}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(DelegatorEvent::ToolResult { call_id, output })
                        if call_id.as_str() == "toolu_1"
                            && output.render_text() == "done"
                            && !output.is_error
                ));
            }
            Err(err) => panic!("fake CLI line should parse: {err}"),
        }
    }

    #[test]
    fn maps_assistant_message_blocks() {
        let mut mapper = ClaudeCodeLineMapper::new();
        let events = mapper.map_line(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"README.md"}}]}}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(DelegatorEvent::AssistantDelta { text }) if text == "hello"
                ));
                assert!(matches!(
                    events.get(1),
                    Some(DelegatorEvent::ToolCall { call_id, name, .. })
                        if call_id.as_str() == "toolu_1" && name == "Read"
                ));
            }
            Err(err) => panic!("assistant line should parse: {err}"),
        }
    }

    #[test]
    fn maps_fake_turn_finish() {
        let mut mapper = ClaudeCodeLineMapper::new();
        let events = mapper.map_line(r#"{"type":"turn_finished","stop_reason":"max_tokens"}"#);
        match events {
            Ok(events) => assert!(matches!(
                events.first(),
                Some(DelegatorEvent::TurnFinished {
                    stop_reason: TurnStopReason::MaxTokens
                })
            )),
            Err(err) => panic!("fake CLI line should parse: {err}"),
        }
    }
}
