use agentloop_contracts::{ToolCallId, ToolOutput, ToolResultBlock, TurnStopReason};
use agentloop_delegator_common::{DelegatorEvent, DelegatorMapError, LineMapper};
use serde::Deserialize;

#[derive(Debug, Default)]
pub struct OpencodeLineMapper;

impl OpencodeLineMapper {
    pub fn new() -> Self {
        Self
    }
}

impl LineMapper for OpencodeLineMapper {
    fn map_line(&mut self, line: &str) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let event: OpencodeWireEvent = serde_json::from_str(trimmed)?;
        Ok(match event {
            OpencodeWireEvent::MessageDelta { text } => {
                vec![DelegatorEvent::AssistantDelta { text }]
            }
            OpencodeWireEvent::ToolCall { id, name, input } => {
                vec![DelegatorEvent::ToolCall {
                    call_id: ToolCallId::from(id),
                    name,
                    args: input,
                }]
            }
            OpencodeWireEvent::ToolResult {
                id,
                output,
                is_error,
            } => vec![DelegatorEvent::ToolResult {
                call_id: ToolCallId::from(id),
                output: tool_output_from_value(output, is_error),
            }],
            OpencodeWireEvent::Done { stop_reason } => vec![DelegatorEvent::TurnFinished {
                stop_reason: map_stop_reason(stop_reason.as_deref()),
            }],
            OpencodeWireEvent::Error { message } => vec![DelegatorEvent::Error { message }],
            OpencodeWireEvent::Unknown => vec![DelegatorEvent::Unknown {
                kind: "unknown".to_owned(),
            }],
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpencodeWireEvent {
    MessageDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    ToolResult {
        id: String,
        #[serde(default)]
        output: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
    Done {
        stop_reason: Option<String>,
    },
    Error {
        message: String,
    },
    #[serde(other)]
    Unknown,
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
    let text = match &value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        _ => match serde_json::to_string(&value) {
            Ok(text) => text,
            Err(_) => value.to_string(),
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

    #[test]
    fn maps_message_delta() {
        let mut mapper = OpencodeLineMapper::new();
        let events = mapper.map_line(r#"{"type":"message_delta","text":"hello"}"#);

        match events {
            Ok(events) => assert_eq!(
                events,
                vec![DelegatorEvent::AssistantDelta {
                    text: "hello".to_owned()
                }]
            ),
            Err(err) => panic!("opencode line should parse: {err}"),
        }
    }

    #[test]
    fn maps_tool_result() {
        let mut mapper = OpencodeLineMapper::new();
        let events =
            mapper.map_line(r#"{"type":"tool_result","id":"toolu_1","output":{"text":"done"}}"#);

        match events {
            Ok(events) => assert!(matches!(
                events.first(),
                Some(DelegatorEvent::ToolResult { call_id, output })
                    if call_id.as_str() == "toolu_1"
                        && output.render_text() == "{\"text\":\"done\"}"
                        && !output.is_error
            )),
            Err(err) => panic!("opencode line should parse: {err}"),
        }
    }

    #[test]
    fn maps_done_stop_reason() {
        let mut mapper = OpencodeLineMapper::new();
        let events = mapper.map_line(r#"{"type":"done","stop_reason":"cancelled"}"#);

        match events {
            Ok(events) => assert!(matches!(
                events.first(),
                Some(DelegatorEvent::TurnFinished {
                    stop_reason: TurnStopReason::Cancelled
                })
            )),
            Err(err) => panic!("opencode line should parse: {err}"),
        }
    }
}
