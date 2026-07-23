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
            OpencodeWireEvent::StepStart => Vec::new(),
            OpencodeWireEvent::Text { part } => {
                if part.text.is_empty() {
                    Vec::new()
                } else {
                    vec![DelegatorEvent::AssistantDelta { text: part.text }]
                }
            }
            OpencodeWireEvent::ToolUse { part } => map_tool_use(part),
            OpencodeWireEvent::StepFinish { part } => {
                let mut events = Vec::new();
                if part.tokens.is_some() || part.cost.is_some() {
                    events.push(DelegatorEvent::Usage {
                        usage: part
                            .tokens
                            .map(OpencodeTokens::into_token_usage)
                            .unwrap_or_default(),
                        cost_usd: part.cost,
                    });
                }
                match part.reason.as_deref() {
                    Some("tool-calls") => {}
                    Some("error") => events.push(DelegatorEvent::TurnFinished {
                        stop_reason: TurnStopReason::Error,
                    }),
                    Some("length") | Some("max_tokens") => {
                        events.push(DelegatorEvent::TurnFinished {
                            stop_reason: TurnStopReason::MaxTokens,
                        });
                    }
                    _ => events.push(DelegatorEvent::TurnFinished {
                        stop_reason: TurnStopReason::EndTurn,
                    }),
                }
                events
            }
            OpencodeWireEvent::Error { message } => vec![DelegatorEvent::Error {
                message: message
                    .unwrap_or_else(|| "opencode reported an error without a message".to_owned()),
            }],
            OpencodeWireEvent::Unknown => vec![DelegatorEvent::Unknown {
                kind: "unknown".to_owned(),
            }],
        })
    }
}

fn map_tool_use(part: OpencodeToolPart) -> Vec<DelegatorEvent> {
    let call_id = part
        .call_id
        .map(ToolCallId::from)
        .unwrap_or_else(ToolCallId::generate);
    let state = part.state.unwrap_or_default();
    let mut events = vec![DelegatorEvent::ToolCall {
        call_id: call_id.clone(),
        name: part.tool,
        args: state.input,
    }];
    match state.status.as_deref() {
        Some("completed") => events.push(DelegatorEvent::ToolResult {
            call_id,
            output: tool_output_from_value(state.output.unwrap_or(serde_json::Value::Null), false),
        }),
        Some("error") => {
            let detail = state
                .error
                .or_else(|| state.output.as_ref().and_then(render_optional))
                .unwrap_or_else(|| "opencode tool call failed".to_owned());
            events.push(DelegatorEvent::ToolResult {
                call_id,
                output: tool_output_from_value(serde_json::Value::String(detail), true),
            });
        }

        _ => {}
    }
    events
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpencodeWireEvent {
    StepStart,
    Text {
        part: OpencodeTextPart,
    },
    ToolUse {
        part: OpencodeToolPart,
    },
    StepFinish {
        part: OpencodeStepFinishPart,
    },
    Error {
        #[serde(default)]
        message: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct OpencodeTextPart {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct OpencodeToolPart {
    tool: String,
    #[serde(rename = "callID")]
    call_id: Option<String>,
    #[serde(default)]
    state: Option<OpencodeToolState>,
}

#[derive(Debug, Default, Deserialize)]
struct OpencodeToolState {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    input: serde_json::Value,
    #[serde(default)]
    output: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpencodeStepFinishPart {
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    tokens: Option<OpencodeTokens>,
    #[serde(default)]
    cost: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct OpencodeTokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    reasoning: u64,
    #[serde(default)]
    cache: OpencodeCacheTokens,
}

#[derive(Debug, Default, Deserialize)]
struct OpencodeCacheTokens {
    #[serde(default)]
    read: u64,
    #[serde(default)]
    write: u64,
}

impl OpencodeTokens {
    fn into_token_usage(self) -> agentloop_contracts::TokenUsage {
        agentloop_contracts::TokenUsage {
            input: self.input,
            output: self.output,
            cache_read: (self.cache.read > 0).then_some(self.cache.read),
            cache_write: (self.cache.write > 0).then_some(self.cache.write),
            reasoning: (self.reasoning > 0).then_some(self.reasoning),
        }
    }
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

    const LIVE_STEP_START: &str = r#"{"type":"step_start","timestamp":1783526453528,"sessionID":"ses_0bd8a4b6bffed7eNJevDYVvE0Z","part":{"id":"prt_f4275f113001mlEBPUAbiarPAS","messageID":"msg_f4275bfc9001gwP911o0G6xiGU","sessionID":"ses_0bd8a4b6bffed7eNJevDYVvE0Z","type":"step-start"}}"#;
    const LIVE_TEXT: &str = r#"{"type":"text","timestamp":1783527293400,"sessionID":"ses_0bd7d6dc2ffeqobEaABDn5cAcJ","part":{"id":"prt_f4282c10b001mKjnNX343Vqe3Y","messageID":"msg_f4282b6b5001R1gHBMrOw16VMI","sessionID":"ses_0bd7d6dc2ffeqobEaABDn5cAcJ","type":"text","text":"The file `note.txt` contains one line:\n\n```\nhello fixture\n```","time":{"start":1783527293195,"end":1783527293397}}}"#;
    const LIVE_TOOL_USE: &str = r#"{"type":"tool_use","timestamp":1783527290547,"sessionID":"ses_0bd7d6dc2ffeqobEaABDn5cAcJ","part":{"type":"tool","tool":"read","callID":"tooluse_fG2C1dGsvmYvhwchLvpwcF","state":{"status":"completed","input":{"filePath":"/work/note.txt"},"output":"<path>/work/note.txt</path>\n<type>file</type>\n<content>\n1: hello fixture\n\n(End of file - total 1 lines)\n</content>","metadata":{"preview":"hello fixture","truncated":false,"loaded":[]},"title":"work/note.txt"}}}"#;
    const LIVE_STEP_FINISH_TOOL_CALLS: &str = r#"{"type":"step_finish","timestamp":1783527290547,"sessionID":"ses_0bd7d6dc2ffeqobEaABDn5cAcJ","part":{"id":"prt_f4282b6b2001bMI4biEKhSOCHE","reason":"tool-calls","messageID":"msg_f42829817001BgkKUuXr7TR0NY","sessionID":"ses_0bd7d6dc2ffeqobEaABDn5cAcJ","type":"step-finish","tokens":{"total":20645,"input":20502,"output":143,"reasoning":0,"cache":{"write":0,"read":0}},"cost":0.063651}}"#;
    const LIVE_STEP_FINISH_STOP: &str = r#"{"type":"step_finish","timestamp":1783527293401,"sessionID":"ses_0bd7d6dc2ffeqobEaABDn5cAcJ","part":{"id":"prt_f4282c1d7001ebrf0yvPPyaXtH","reason":"stop","messageID":"msg_f4282b6b5001R1gHBMrOw16VMI","sessionID":"ses_0bd7d6dc2ffeqobEaABDn5cAcJ","type":"step-finish","tokens":{"total":20790,"input":270,"output":21,"reasoning":0,"cache":{"write":0,"read":20499}},"cost":0.0072747}}"#;

    #[test]
    fn step_start_maps_to_nothing() {
        let mut mapper = OpencodeLineMapper::new();
        match mapper.map_line(LIVE_STEP_START) {
            Ok(events) => assert!(events.is_empty()),
            Err(err) => panic!("recorded line should parse: {err}"),
        }
    }

    #[test]
    fn text_part_maps_to_assistant_delta() {
        let mut mapper = OpencodeLineMapper::new();
        match mapper.map_line(LIVE_TEXT) {
            Ok(events) => assert_eq!(
                events,
                vec![DelegatorEvent::AssistantDelta {
                    text: "The file `note.txt` contains one line:\n\n```\nhello fixture\n```"
                        .to_owned()
                }]
            ),
            Err(err) => panic!("recorded line should parse: {err}"),
        }
    }

    #[test]
    fn completed_tool_use_maps_to_call_and_result() {
        let mut mapper = OpencodeLineMapper::new();
        let events = match mapper.map_line(LIVE_TOOL_USE) {
            Ok(events) => events,
            Err(err) => panic!("recorded line should parse: {err}"),
        };
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            DelegatorEvent::ToolCall { call_id, name, args }
                if call_id.as_str() == "tooluse_fG2C1dGsvmYvhwchLvpwcF"
                    && name == "read"
                    && args["filePath"] == "/work/note.txt"
        ));
        assert!(matches!(
            &events[1],
            DelegatorEvent::ToolResult { call_id, output }
                if call_id.as_str() == "tooluse_fG2C1dGsvmYvhwchLvpwcF"
                    && !output.is_error
                    && output.render_text().contains("hello fixture")
        ));
    }

    #[test]
    fn mid_turn_step_finish_reports_usage_without_finishing() {
        let mut mapper = OpencodeLineMapper::new();
        let events = match mapper.map_line(LIVE_STEP_FINISH_TOOL_CALLS) {
            Ok(events) => events,
            Err(err) => panic!("recorded line should parse: {err}"),
        };
        assert_eq!(
            events,
            vec![DelegatorEvent::Usage {
                usage: agentloop_contracts::TokenUsage {
                    input: 20502,
                    output: 143,
                    cache_read: None,
                    cache_write: None,
                    reasoning: None,
                },
                cost_usd: Some(0.063651),
            }]
        );
    }

    #[test]
    fn stop_step_finish_reports_usage_and_finishes_turn() {
        let mut mapper = OpencodeLineMapper::new();
        let events = match mapper.map_line(LIVE_STEP_FINISH_STOP) {
            Ok(events) => events,
            Err(err) => panic!("recorded line should parse: {err}"),
        };
        assert_eq!(
            events,
            vec![
                DelegatorEvent::Usage {
                    usage: agentloop_contracts::TokenUsage {
                        input: 270,
                        output: 21,
                        cache_read: Some(20499),
                        cache_write: None,
                        reasoning: None,
                    },
                    cost_usd: Some(0.0072747),
                },
                DelegatorEvent::TurnFinished {
                    stop_reason: TurnStopReason::EndTurn
                },
            ]
        );
    }

    #[test]
    fn unknown_event_types_are_routed_to_unknown() {
        let mut mapper = OpencodeLineMapper::new();
        match mapper.map_line(r#"{"type":"totally_new_frame","part":{}}"#) {
            Ok(events) => assert!(matches!(
                events.first(),
                Some(DelegatorEvent::Unknown { .. })
            )),
            Err(err) => panic!("unknown frames must not error: {err}"),
        }
    }
}
