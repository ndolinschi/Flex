use agentloop_contracts::{ToolCallId, ToolOutput, ToolResultBlock, TurnStopReason};
use agentloop_delegator_common::{DelegatorEvent, DelegatorMapError, LineMapper};
use serde::Deserialize;

use crate::protocol::AcpNotification;

#[derive(Debug, Default)]
pub struct AcpLineMapper;

impl AcpLineMapper {
    pub fn new() -> Self {
        Self
    }
}

impl LineMapper for AcpLineMapper {
    fn map_line(&mut self, line: &str) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let notification: AcpNotification = serde_json::from_str(trimmed)?;
        if notification.method != "session/update" {
            return Ok(vec![DelegatorEvent::Unknown {
                kind: notification.method,
            }]);
        }

        let params = notification
            .params
            .ok_or(DelegatorMapError::MissingField("params"))?;
        let update: AcpSessionUpdate = serde_json::from_value(params)?;
        Ok(match update {
            AcpSessionUpdate::AssistantDelta { text } => {
                vec![DelegatorEvent::AssistantDelta { text }]
            }
            AcpSessionUpdate::ToolCall { id, name, input } => {
                vec![DelegatorEvent::ToolCall {
                    call_id: ToolCallId::from(id),
                    name,
                    args: input,
                }]
            }
            AcpSessionUpdate::ToolResult {
                id,
                content,
                is_error,
            } => vec![DelegatorEvent::ToolResult {
                call_id: ToolCallId::from(id),
                output: tool_output_from_value(content, is_error),
            }],
            AcpSessionUpdate::TurnFinished { stop_reason } => {
                vec![DelegatorEvent::TurnFinished {
                    stop_reason: map_stop_reason(stop_reason.as_deref()),
                }]
            }
            AcpSessionUpdate::Error { message } => vec![DelegatorEvent::Error { message }],
            AcpSessionUpdate::Unknown => vec![DelegatorEvent::Unknown {
                kind: "session/update".to_owned(),
            }],
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AcpSessionUpdate {
    AssistantDelta {
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
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
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
    fn maps_session_update_delta() {
        let mut mapper = AcpLineMapper::new();
        let events = mapper.map_line(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"assistant_delta","text":"hi"}}"#,
        );

        match events {
            Ok(events) => assert_eq!(
                events,
                vec![DelegatorEvent::AssistantDelta {
                    text: "hi".to_owned()
                }]
            ),
            Err(err) => panic!("ACP notification should parse: {err}"),
        }
    }

    #[test]
    fn maps_session_update_tool_call() {
        let mut mapper = AcpLineMapper::new();
        let events = mapper.map_line(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"kind":"tool_call","id":"toolu_1","name":"Read","input":{"path":"README.md"}}}"#,
        );

        match events {
            Ok(events) => assert!(matches!(
                events.first(),
                Some(DelegatorEvent::ToolCall { call_id, name, args })
                    if call_id.as_str() == "toolu_1"
                        && name == "Read"
                        && args["path"] == "README.md"
            )),
            Err(err) => panic!("ACP notification should parse: {err}"),
        }
    }

    #[test]
    fn unknown_method_stays_unknown() {
        let mut mapper = AcpLineMapper::new();
        let events = mapper.map_line(r#"{"jsonrpc":"2.0","method":"telemetry/event"}"#);

        match events {
            Ok(events) => assert_eq!(
                events,
                vec![DelegatorEvent::Unknown {
                    kind: "telemetry/event".to_owned()
                }]
            ),
            Err(err) => panic!("unknown method should still parse: {err}"),
        }
    }
}
