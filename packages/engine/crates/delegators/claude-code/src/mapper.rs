use agentloop_contracts::{ToolCallId, ToolOutput, ToolResultBlock, TurnStopReason};
use agentloop_delegator_common::{DelegatorEvent, DelegatorMapError, LineMapper};
use serde::Deserialize;

#[derive(Debug, Default)]
pub struct ClaudeCodeLineMapper {
    /// Claude Code emits complete `assistant` messages *and* echoes the final
    /// text again in its `result` frame. Once assistant text has streamed,
    /// the result echo must be suppressed or the message doubles.
    saw_assistant_text: bool,
}

impl ClaudeCodeLineMapper {
    pub fn new() -> Self {
        Self::default()
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
                self.saw_assistant_text = true;
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
            ClaudeCodeWireEvent::Assistant { message } => {
                let events = map_assistant_message(message)?;
                if events
                    .iter()
                    .any(|event| matches!(event, DelegatorEvent::AssistantDelta { .. }))
                {
                    self.saw_assistant_text = true;
                }
                events
            }
            ClaudeCodeWireEvent::Result {
                result,
                is_error,
                usage,
                total_cost_usd,
            } => {
                let mut events = Vec::new();
                // The result frame echoes the final assistant text; only use
                // it when no assistant message streamed it already.
                if !self.saw_assistant_text {
                    if let Some(result) = result.filter(|value| !value.is_empty()) {
                        events.push(DelegatorEvent::AssistantDelta { text: result });
                    }
                }
                if usage.is_some() || total_cost_usd.is_some() {
                    events.push(DelegatorEvent::Usage {
                        usage: usage.map(ClaudeUsage::into_token_usage).unwrap_or_default(),
                        cost_usd: total_cost_usd,
                    });
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
                self.saw_assistant_text = false;
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
        #[serde(default)]
        usage: Option<ClaudeUsage>,
        #[serde(default)]
        total_cost_usd: Option<f64>,
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

#[derive(Debug, Default, Deserialize)]
struct ClaudeUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

impl ClaudeUsage {
    fn into_token_usage(self) -> agentloop_contracts::TokenUsage {
        agentloop_contracts::TokenUsage {
            input: self.input_tokens,
            output: self.output_tokens,
            cache_read: self.cache_read_input_tokens,
            cache_write: self.cache_creation_input_tokens,
            reasoning: None,
        }
    }
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
    fn result_echo_is_deduped_and_usage_mapped() {
        let mut mapper = ClaudeCodeLineMapper::new();
        // The assistant message streams the final text...
        let events = mapper
            .map_line(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"pong"}]}}"#,
            )
            .expect("assistant line maps");
        assert_eq!(
            events,
            vec![DelegatorEvent::AssistantDelta {
                text: "pong".to_owned()
            }]
        );
        // ...and the result frame echoes it. The echo must be suppressed,
        // while usage and cost are captured.
        let events = mapper
            .map_line(
                r#"{"type":"result","result":"pong","is_error":false,"usage":{"input_tokens":7,"output_tokens":3,"cache_read_input_tokens":100},"total_cost_usd":0.0125}"#,
            )
            .expect("result line maps");
        assert_eq!(
            events,
            vec![
                DelegatorEvent::Usage {
                    usage: agentloop_contracts::TokenUsage {
                        input: 7,
                        output: 3,
                        cache_read: Some(100),
                        cache_write: None,
                        reasoning: None,
                    },
                    cost_usd: Some(0.0125),
                },
                DelegatorEvent::TurnFinished {
                    stop_reason: TurnStopReason::EndTurn
                },
            ]
        );
    }

    #[test]
    fn result_text_is_used_when_nothing_streamed() {
        let mut mapper = ClaudeCodeLineMapper::new();
        let events = mapper
            .map_line(r#"{"type":"result","result":"pong","is_error":false}"#)
            .expect("result line maps");
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
