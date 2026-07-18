use agentloop_contracts::{ToolCallId, ToolOutput, ToolResultBlock, TurnStopReason};
use agentloop_delegator_common::{DelegatorEvent, DelegatorMapError, LineMapper};
use serde::Deserialize;

/// Maps `grok -p --output-format streaming-json` lines into normalized events.
///
/// Wire shape is modeled on the documented headless NDJSON stream (status /
/// tool_call / tool_result / file_edit / complete / text / error). Fixtures
/// below are **UNVERIFIED** — recorded from public docs/blog samples, not a
/// live `grok` CLI capture (no binary on PATH in this environment).
#[derive(Debug, Default)]
pub struct GrokLineMapper {
    /// Open tool calls awaiting a matching `tool_result` (id + tool name).
    pending: Vec<(ToolCallId, String)>,
}

impl GrokLineMapper {
    pub fn new() -> Self {
        Self::default()
    }

    fn push_tool_call(
        &mut self,
        name: String,
        args: serde_json::Value,
        call_id: Option<String>,
    ) -> DelegatorEvent {
        let call_id = call_id
            .map(ToolCallId::from)
            .unwrap_or_else(ToolCallId::generate);
        self.pending.push((call_id.clone(), name.clone()));
        DelegatorEvent::ToolCall {
            call_id,
            name,
            args,
        }
    }

    fn take_pending(&mut self, tool: Option<&str>) -> ToolCallId {
        if let Some(tool) = tool {
            if let Some(idx) = self
                .pending
                .iter()
                .rposition(|(_, name)| name.as_str() == tool)
            {
                return self.pending.remove(idx).0;
            }
        }
        self.pending
            .pop()
            .map(|(id, _)| id)
            .unwrap_or_else(ToolCallId::generate)
    }
}

impl LineMapper for GrokLineMapper {
    fn map_line(&mut self, line: &str) -> Result<Vec<DelegatorEvent>, DelegatorMapError> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let event: GrokWireEvent = serde_json::from_str(trimmed)?;
        Ok(match event {
            // Progress chatter — nothing to surface in the transcript.
            GrokWireEvent::Status => Vec::new(),
            GrokWireEvent::Text { text, message } => {
                let text = text.or(message).unwrap_or_default();
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![DelegatorEvent::AssistantDelta { text }]
                }
            }
            GrokWireEvent::ToolCall {
                tool,
                path,
                args,
                id,
                call_id,
            } => {
                let name = tool.unwrap_or_else(|| "unknown".to_owned());
                let mut value = args.unwrap_or_else(|| serde_json::json!({}));
                if let Some(path) = path {
                    if let Some(obj) = value.as_object_mut() {
                        obj.entry("path".to_owned())
                            .or_insert(serde_json::Value::String(path));
                    }
                }
                vec![self.push_tool_call(name, value, id.or(call_id))]
            }
            GrokWireEvent::ToolResult {
                tool,
                success,
                output,
                content,
                error,
                message,
            } => {
                let call_id = self.take_pending(tool.as_deref());
                let is_error = success == Some(false) || error.is_some();
                let detail = error
                    .or(message)
                    .or_else(|| content.and_then(|v| render_optional(&v)))
                    .or_else(|| output.and_then(|v| render_optional(&v)))
                    .unwrap_or_else(|| {
                        if is_error {
                            "grok tool call failed".to_owned()
                        } else {
                            "ok".to_owned()
                        }
                    });
                vec![DelegatorEvent::ToolResult {
                    call_id,
                    output: tool_output_from_value(serde_json::Value::String(detail), is_error),
                }]
            }
            GrokWireEvent::FileEdit { path, action } => {
                let name = "file_edit".to_owned();
                let args = serde_json::json!({
                    "path": path,
                    "action": action,
                });
                let call = self.push_tool_call(name.clone(), args, None);
                let call_id = self.take_pending(Some(name.as_str()));
                let summary = match (path.as_deref(), action.as_deref()) {
                    (Some(path), Some(action)) => format!("{action} {path}"),
                    (Some(path), None) => path.to_owned(),
                    _ => "file edit".to_owned(),
                };
                vec![
                    call,
                    DelegatorEvent::ToolResult {
                        call_id,
                        output: tool_output_from_value(serde_json::Value::String(summary), false),
                    },
                ]
            }
            GrokWireEvent::Complete {
                tokens_used,
                cost,
                text,
                message,
                success,
            } => {
                let mut events = Vec::new();
                if let Some(text) = text.or(message).filter(|value| !value.is_empty()) {
                    events.push(DelegatorEvent::AssistantDelta { text });
                }
                if tokens_used.is_some() || cost.is_some() {
                    events.push(DelegatorEvent::Usage {
                        // Blog samples expose a single `tokens_used` total with
                        // no input/output split — stash it in `output`.
                        usage: agentloop_contracts::TokenUsage {
                            input: 0,
                            output: tokens_used.unwrap_or(0),
                            cache_read: None,
                            cache_write: None,
                            reasoning: None,
                        },
                        cost_usd: cost,
                    });
                }
                events.push(DelegatorEvent::TurnFinished {
                    stop_reason: if success == Some(false) {
                        TurnStopReason::Error
                    } else {
                        TurnStopReason::EndTurn
                    },
                });
                self.pending.clear();
                events
            }
            GrokWireEvent::Error { message, error } => vec![DelegatorEvent::Error {
                message: message
                    .or(error)
                    .unwrap_or_else(|| "grok reported an error without a message".to_owned()),
            }],
            GrokWireEvent::Unknown => vec![DelegatorEvent::Unknown {
                kind: "unknown".to_owned(),
            }],
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum GrokWireEvent {
    Status,
    Text {
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        message: Option<String>,
    },
    ToolCall {
        #[serde(default)]
        tool: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        args: Option<serde_json::Value>,
        #[serde(default)]
        id: Option<String>,
        #[serde(default, rename = "call_id")]
        call_id: Option<String>,
    },
    ToolResult {
        #[serde(default)]
        tool: Option<String>,
        #[serde(default)]
        success: Option<bool>,
        #[serde(default)]
        output: Option<serde_json::Value>,
        #[serde(default)]
        content: Option<serde_json::Value>,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        message: Option<String>,
    },
    FileEdit {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        action: Option<String>,
    },
    Complete {
        #[serde(default)]
        tokens_used: Option<u64>,
        #[serde(default)]
        cost: Option<f64>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        success: Option<bool>,
    },
    Error {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        error: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

fn render_optional(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Null => None,
        other => serde_json::to_string(other).ok(),
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

    // UNVERIFIED: recorded from docs/blog samples, not a live CLI.
    // https://www.aimadetools.com/blog/grok-build-headless-ci-automation/
    // https://docs.x.ai/build/cli/headless-scripting
    const DOC_STATUS: &str = r#"{"type":"status","message":"Reading src/auth.ts..."}"#;
    const DOC_TOOL_CALL: &str = r#"{"type":"tool_call","tool":"file_read","path":"src/auth.ts"}"#;
    const DOC_TOOL_RESULT: &str = r#"{"type":"tool_result","tool":"file_read","success":true}"#;
    const DOC_FILE_EDIT: &str =
        r#"{"type":"file_edit","path":"src/auth.test.ts","action":"create"}"#;
    const DOC_COMPLETE: &str =
        r#"{"type":"complete","files_modified":["src/auth.test.ts"],"tokens_used":12450}"#;
    const DOC_TEXT: &str = r#"{"type":"text","text":"pong"}"#;
    const DOC_ERROR: &str = r#"{"type":"error","message":"boom"}"#;

    #[test]
    fn status_maps_to_nothing() {
        let mut mapper = GrokLineMapper::new();
        match mapper.map_line(DOC_STATUS) {
            Ok(events) => assert!(events.is_empty()),
            Err(err) => panic!("doc line should parse: {err}"),
        }
    }

    #[test]
    fn text_maps_to_assistant_delta() {
        let mut mapper = GrokLineMapper::new();
        match mapper.map_line(DOC_TEXT) {
            Ok(events) => assert_eq!(
                events,
                vec![DelegatorEvent::AssistantDelta {
                    text: "pong".to_owned()
                }]
            ),
            Err(err) => panic!("doc line should parse: {err}"),
        }
    }

    #[test]
    fn tool_call_and_result_pair() {
        let mut mapper = GrokLineMapper::new();
        let call = match mapper.map_line(DOC_TOOL_CALL) {
            Ok(events) => events,
            Err(err) => panic!("doc line should parse: {err}"),
        };
        assert_eq!(call.len(), 1);
        let call_id = match &call[0] {
            DelegatorEvent::ToolCall {
                call_id,
                name,
                args,
            } => {
                assert_eq!(name, "file_read");
                assert_eq!(args["path"], "src/auth.ts");
                call_id.clone()
            }
            other => panic!("expected ToolCall, got {other:?}"),
        };

        let result = match mapper.map_line(DOC_TOOL_RESULT) {
            Ok(events) => events,
            Err(err) => panic!("doc line should parse: {err}"),
        };
        assert!(matches!(
            &result[..],
            [DelegatorEvent::ToolResult { call_id: id, output }]
                if *id == call_id && !output.is_error
        ));
    }

    #[test]
    fn file_edit_emits_call_and_result() {
        let mut mapper = GrokLineMapper::new();
        let events = match mapper.map_line(DOC_FILE_EDIT) {
            Ok(events) => events,
            Err(err) => panic!("doc line should parse: {err}"),
        };
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            DelegatorEvent::ToolCall { name, args, .. }
                if name == "file_edit"
                    && args["path"] == "src/auth.test.ts"
                    && args["action"] == "create"
        ));
        assert!(matches!(
            &events[1],
            DelegatorEvent::ToolResult { output, .. }
                if !output.is_error && output.render_text().contains("src/auth.test.ts")
        ));
    }

    #[test]
    fn complete_reports_usage_and_finishes() {
        let mut mapper = GrokLineMapper::new();
        let events = match mapper.map_line(DOC_COMPLETE) {
            Ok(events) => events,
            Err(err) => panic!("doc line should parse: {err}"),
        };
        assert_eq!(
            events,
            vec![
                DelegatorEvent::Usage {
                    usage: agentloop_contracts::TokenUsage {
                        input: 0,
                        output: 12450,
                        cache_read: None,
                        cache_write: None,
                        reasoning: None,
                    },
                    cost_usd: None,
                },
                DelegatorEvent::TurnFinished {
                    stop_reason: TurnStopReason::EndTurn,
                },
            ]
        );
    }

    #[test]
    fn error_maps_to_error_event() {
        let mut mapper = GrokLineMapper::new();
        match mapper.map_line(DOC_ERROR) {
            Ok(events) => assert_eq!(
                events,
                vec![DelegatorEvent::Error {
                    message: "boom".to_owned()
                }]
            ),
            Err(err) => panic!("doc line should parse: {err}"),
        }
    }

    #[test]
    fn unknown_type_maps_to_unknown() {
        let mut mapper = GrokLineMapper::new();
        match mapper.map_line(r#"{"type":"future_frame","x":1}"#) {
            Ok(events) => assert!(matches!(&events[..], [DelegatorEvent::Unknown { .. }])),
            Err(err) => panic!("unknown frames must not fail parse: {err}"),
        }
    }
}
