//! Bedrock Converse request building and stream-event mapping.

use std::collections::BTreeMap;

use agentloop_contracts::{
    ContentBlock, Message, MessageId, ModelInfo, Role, StopReason, TokenUsage, ToolCallId,
    ToolResultBlock,
};
use agentloop_core::{ChatRequest, ProviderStreamEvent, ToolChoice, ToolSpec};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct ConverseRequest {
    messages: Vec<BedrockMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    system: Vec<SystemContent>,
    #[serde(rename = "inferenceConfig", skip_serializing_if = "Option::is_none")]
    inference_config: Option<InferenceConfig>,
    #[serde(rename = "toolConfig", skip_serializing_if = "Option::is_none")]
    tool_config: Option<ToolConfig>,
    #[serde(
        rename = "additionalModelRequestFields",
        skip_serializing_if = "Option::is_none"
    )]
    additional_model_request_fields: Option<AdditionalModelRequestFields>,
}

/// Model-specific passthrough — Claude-on-Bedrock reads `thinking` the same
/// way direct Anthropic does (`{"type": "enabled", "budget_tokens": N}`), just
/// nested under this Converse-API envelope instead of a top-level field.
#[derive(Debug, Serialize)]
struct AdditionalModelRequestFields {
    thinking: BedrockThinking,
}

#[derive(Debug, Serialize)]
struct BedrockThinking {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(rename = "budget_tokens")]
    budget_tokens: u32,
}

#[derive(Debug, Serialize)]
struct SystemContent {
    text: String,
}

#[derive(Debug, Serialize)]
struct BedrockMessage {
    role: String,
    content: Vec<ContentItem>,
}

/// A Converse content block: exactly one key per item.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ContentItem {
    Text {
        text: String,
    },
    ToolUse {
        #[serde(rename = "toolUse")]
        tool_use: ToolUseWire,
    },
    ToolResult {
        #[serde(rename = "toolResult")]
        tool_result: ToolResultWire,
    },
}

#[derive(Debug, Serialize)]
struct ToolUseWire {
    #[serde(rename = "toolUseId")]
    tool_use_id: String,
    name: String,
    input: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ToolResultWire {
    #[serde(rename = "toolUseId")]
    tool_use_id: String,
    content: Vec<ToolResultContent>,
    status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ToolResultContent {
    Text { text: String },
    Json { json: serde_json::Value },
}

#[derive(Debug, Serialize)]
struct InferenceConfig {
    #[serde(rename = "maxTokens", skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ToolConfig {
    tools: Vec<ToolWrapper>,
    #[serde(rename = "toolChoice", skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct ToolWrapper {
    #[serde(rename = "toolSpec")]
    tool_spec: ToolSpecWire,
}

#[derive(Debug, Serialize)]
struct ToolSpecWire {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: InputSchema,
}

#[derive(Debug, Serialize)]
struct InputSchema {
    json: serde_json::Value,
}

/// Build a Converse request body from the canonical [`ChatRequest`]. The model
/// id goes in the URL path, not the body.
pub(crate) fn build_request(request: ChatRequest) -> ConverseRequest {
    let system = request
        .system
        .filter(|text| !text.trim().is_empty())
        .map(|text| vec![SystemContent { text }])
        .unwrap_or_default();

    // Extended thinking and `temperature` are mutually exclusive on Claude's
    // Converse API (same constraint as direct Anthropic) — drop temperature
    // whenever thinking is requested rather than let the call fail upstream.
    let temperature = request.temperature.filter(|_| request.thinking.is_none());
    let inference_config =
        (request.max_tokens.is_some() || temperature.is_some()).then_some(InferenceConfig {
            max_tokens: request.max_tokens,
            temperature,
        });

    let additional_model_request_fields =
        request
            .thinking
            .map(|thinking| AdditionalModelRequestFields {
                thinking: BedrockThinking {
                    kind: "enabled",
                    budget_tokens: thinking.budget_tokens,
                },
            });

    ConverseRequest {
        messages: build_messages(request.messages),
        system,
        inference_config,
        tool_config: build_tool_config(request.tools, request.tool_choice),
        additional_model_request_fields,
    }
}

fn build_messages(messages: Vec<Message>) -> Vec<BedrockMessage> {
    let mut out = Vec::new();
    for message in messages {
        let mut content = Vec::new();
        for block in message.content {
            match block {
                ContentBlock::Markdown { text } if !text.is_empty() => {
                    content.push(ContentItem::Text { text });
                }
                ContentBlock::ToolUse { id, name, input } => {
                    content.push(ContentItem::ToolUse {
                        tool_use: ToolUseWire {
                            tool_use_id: id.to_string(),
                            name,
                            input: normalize_input(input),
                        },
                    });
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content: blocks,
                    is_error,
                } => {
                    content.push(ContentItem::ToolResult {
                        tool_result: ToolResultWire {
                            tool_use_id: tool_use_id.to_string(),
                            content: tool_result_content(blocks),
                            status: if is_error { "error" } else { "success" },
                        },
                    });
                }
                _ => {}
            }
        }
        if !content.is_empty() {
            out.push(BedrockMessage {
                role: role_name(message.role).to_owned(),
                content,
            });
        }
    }
    out
}

/// Bedrock requires tool input to be a JSON object; coerce a null/absent input
/// to `{}`.
fn normalize_input(input: serde_json::Value) -> serde_json::Value {
    if input.is_null() {
        serde_json::json!({})
    } else {
        input
    }
}

fn tool_result_content(blocks: Vec<ToolResultBlock>) -> Vec<ToolResultContent> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            ToolResultBlock::Markdown { text } => out.push(ToolResultContent::Text { text }),
            ToolResultBlock::Json { value } => out.push(ToolResultContent::Json { json: value }),
            _ => {}
        }
    }
    if out.is_empty() {
        out.push(ToolResultContent::Text {
            text: "(no output)".to_owned(),
        });
    }
    out
}

fn build_tool_config(tools: Vec<ToolSpec>, tool_choice: ToolChoice) -> Option<ToolConfig> {
    if matches!(tool_choice, ToolChoice::None) || tools.is_empty() {
        return None;
    }
    let tools: Vec<ToolWrapper> = tools
        .into_iter()
        .filter(|tool| match &tool_choice {
            ToolChoice::Named(name) => &tool.name == name,
            _ => true,
        })
        .map(|tool| ToolWrapper {
            tool_spec: ToolSpecWire {
                name: tool.name,
                description: tool.description,
                input_schema: InputSchema {
                    json: tool.input_schema,
                },
            },
        })
        .collect();
    if tools.is_empty() {
        return None;
    }
    let choice = match &tool_choice {
        ToolChoice::Required => Some(serde_json::json!({ "any": {} })),
        ToolChoice::Named(name) => Some(serde_json::json!({ "tool": { "name": name } })),
        _ => None,
    };
    Some(ToolConfig {
        tools,
        tool_choice: choice,
    })
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Assistant => "assistant",
        _ => "user",
    }
}

/// Maps decoded Converse stream events onto the canonical event stream.
#[derive(Debug)]
pub(crate) struct ConverseStreamMapper {
    requested_model: String,
    message_started: bool,
    tool_calls: BTreeMap<u64, ToolCallId>,
    stop_reason: StopReason,
    ended: bool,
}

impl ConverseStreamMapper {
    pub(crate) fn new(requested_model: impl Into<String>) -> Self {
        Self {
            requested_model: requested_model.into(),
            message_started: false,
            tool_calls: BTreeMap::new(),
            stop_reason: StopReason::EndTurn,
            ended: false,
        }
    }

    pub(crate) fn ended(&self) -> bool {
        self.ended
    }

    /// The stop reason to use if the stream ends without a `metadata` event.
    pub(crate) fn stop_reason(&self) -> StopReason {
        self.stop_reason
    }

    pub(crate) fn map_event(
        &mut self,
        event_type: &str,
        payload: &[u8],
    ) -> Result<Vec<ProviderStreamEvent>, serde_json::Error> {
        let value: serde_json::Value = if payload.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(payload)?
        };
        let mut events = Vec::new();
        match event_type {
            "messageStart" => self.start_message(&mut events),
            "contentBlockStart" => {
                self.start_message(&mut events);
                let index = block_index(&value);
                if let Some(tool_use) = value.get("start").and_then(|s| s.get("toolUse")) {
                    let call_id = ToolCallId::generate();
                    self.tool_calls.insert(index, call_id.clone());
                    events.push(ProviderStreamEvent::ToolCallStart {
                        call_id,
                        name: string_at(tool_use, "name"),
                    });
                }
            }
            "contentBlockDelta" => {
                self.start_message(&mut events);
                if let Some(delta) = value.get("delta") {
                    if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            events.push(ProviderStreamEvent::MarkdownDelta {
                                text: text.to_owned(),
                            });
                        }
                    }
                    if let Some(reasoning) = delta.get("reasoningContent") {
                        if let Some(text) = reasoning.get("text").and_then(|t| t.as_str()) {
                            events.push(ProviderStreamEvent::ThinkingDelta {
                                text: text.to_owned(),
                            });
                        }
                        if let Some(sig) = reasoning.get("signature").and_then(|s| s.as_str()) {
                            events.push(ProviderStreamEvent::ThinkingSignature {
                                signature: sig.to_owned(),
                            });
                        }
                    }
                    if let Some(input) = delta.get("toolUse").and_then(|t| t.get("input")) {
                        if let Some(call_id) = self.tool_calls.get(&block_index(&value)) {
                            events.push(ProviderStreamEvent::ToolCallArgsDelta {
                                call_id: call_id.clone(),
                                json_fragment: input
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| input.to_string()),
                            });
                        }
                    }
                }
            }
            "contentBlockStop" => {
                if let Some(call_id) = self.tool_calls.remove(&block_index(&value)) {
                    events.push(ProviderStreamEvent::ToolCallEnd { call_id });
                }
            }
            "messageStop" => {
                self.stop_reason =
                    map_stop_reason(value.get("stopReason").and_then(|s| s.as_str()));
            }
            "metadata" => {
                if let Some(usage) = value.get("usage") {
                    events.push(ProviderStreamEvent::Usage(usage_from(usage)));
                }
                events.push(ProviderStreamEvent::MessageEnd {
                    stop_reason: self.stop_reason,
                });
                self.ended = true;
            }
            _ => {}
        }
        Ok(events)
    }

    fn start_message(&mut self, events: &mut Vec<ProviderStreamEvent>) {
        if self.message_started {
            return;
        }
        events.push(ProviderStreamEvent::MessageStart {
            message_id: MessageId::generate(),
            model: self.requested_model.clone(),
        });
        self.message_started = true;
    }
}

fn block_index(value: &serde_json::Value) -> u64 {
    value
        .get("contentBlockIndex")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

fn string_at(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn usage_from(usage: &serde_json::Value) -> TokenUsage {
    let field = |key: &str| usage.get(key).and_then(|v| v.as_u64());
    TokenUsage {
        input: field("inputTokens").unwrap_or(0),
        output: field("outputTokens").unwrap_or(0),
        cache_read: field("cacheReadInputTokens"),
        cache_write: field("cacheWriteInputTokens"),
        reasoning: None,
    }
}

fn map_stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("guardrail_intervened") | Some("content_filtered") => StopReason::Refusal,
        _ => StopReason::EndTurn,
    }
}

/// A curated set of common Bedrock model ids for the picker. Bedrock has no
/// bearer-token model-list endpoint, so this is static; any id can still be set
/// explicitly via `/model`.
pub(crate) fn static_models() -> Vec<ModelInfo> {
    const MODELS: &[(&str, &str, u32, bool)] = &[
        (
            "anthropic.claude-3-5-sonnet-20241022-v2:0",
            "Claude 3.5 Sonnet v2",
            200_000,
            true,
        ),
        (
            "anthropic.claude-3-5-haiku-20241022-v1:0",
            "Claude 3.5 Haiku",
            200_000,
            false,
        ),
        (
            "anthropic.claude-3-haiku-20240307-v1:0",
            "Claude 3 Haiku",
            200_000,
            false,
        ),
        (
            "meta.llama3-3-70b-instruct-v1:0",
            "Llama 3.3 70B Instruct",
            128_000,
            false,
        ),
        ("amazon.nova-pro-v1:0", "Amazon Nova Pro", 300_000, false),
        ("amazon.nova-lite-v1:0", "Amazon Nova Lite", 300_000, false),
    ];
    MODELS
        .iter()
        .map(|(id, name, ctx, reasoning)| ModelInfo {
            id: (*id).to_owned(),
            display_name: Some((*name).to_owned()),
            context_window: Some(*ctx),
            reasoning: *reasoning,
            vision: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::ContentBlock;

    fn frame_json(
        mapper: &mut ConverseStreamMapper,
        ty: &str,
        json: &str,
    ) -> Vec<ProviderStreamEvent> {
        mapper.map_event(ty, json.as_bytes()).expect("valid json")
    }

    #[test]
    fn maps_text_stream() {
        let mut mapper = ConverseStreamMapper::new("claude-test");
        let start = frame_json(&mut mapper, "messageStart", r#"{"role":"assistant"}"#);
        assert!(matches!(
            start.first(),
            Some(ProviderStreamEvent::MessageStart { .. })
        ));
        let delta = frame_json(
            &mut mapper,
            "contentBlockDelta",
            r#"{"contentBlockIndex":0,"delta":{"text":"hello"}}"#,
        );
        assert!(matches!(
            delta.first(),
            Some(ProviderStreamEvent::MarkdownDelta { text }) if text == "hello"
        ));
        frame_json(&mut mapper, "messageStop", r#"{"stopReason":"end_turn"}"#);
        let meta = frame_json(
            &mut mapper,
            "metadata",
            r#"{"usage":{"inputTokens":10,"outputTokens":5}}"#,
        );
        assert!(matches!(
            meta.first(),
            Some(ProviderStreamEvent::Usage(TokenUsage {
                input: 10,
                output: 5,
                ..
            }))
        ));
        assert!(matches!(
            meta.get(1),
            Some(ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::EndTurn
            })
        ));
        assert!(mapper.ended());
    }

    #[test]
    fn maps_tool_use_stream() {
        let mut mapper = ConverseStreamMapper::new("claude-test");
        frame_json(&mut mapper, "messageStart", r#"{"role":"assistant"}"#);
        let start = frame_json(
            &mut mapper,
            "contentBlockStart",
            r#"{"contentBlockIndex":1,"start":{"toolUse":{"toolUseId":"tu_1","name":"Read"}}}"#,
        );
        assert!(start.iter().any(|e| matches!(
            e, ProviderStreamEvent::ToolCallStart { name, .. } if name == "Read"
        )));
        let delta = frame_json(
            &mut mapper,
            "contentBlockDelta",
            r#"{"contentBlockIndex":1,"delta":{"toolUse":{"input":"{\"file_path\":\"a\"}"}}}"#,
        );
        assert!(delta.iter().any(|e| matches!(
            e, ProviderStreamEvent::ToolCallArgsDelta { json_fragment, .. }
                if json_fragment == "{\"file_path\":\"a\"}"
        )));
        let stop = frame_json(
            &mut mapper,
            "contentBlockStop",
            r#"{"contentBlockIndex":1}"#,
        );
        assert!(
            stop.iter()
                .any(|e| matches!(e, ProviderStreamEvent::ToolCallEnd { .. }))
        );
        frame_json(&mut mapper, "messageStop", r#"{"stopReason":"tool_use"}"#);
        assert_eq!(mapper.stop_reason(), StopReason::ToolUse);
    }

    #[test]
    fn builds_converse_request_with_tool_result() {
        let request = ChatRequest {
            system: Some("be helpful".to_owned()),
            max_tokens: Some(1024),
            temperature: Some(0.2),
            ..ChatRequest::new(
                "anthropic.claude-3-5-sonnet-20241022-v2:0",
                vec![Message {
                    role: Role::User,
                    content: vec![ContentBlock::ToolResult {
                        tool_use_id: ToolCallId::from("tu_1"),
                        content: vec![ToolResultBlock::markdown("done")],
                        is_error: false,
                    }],
                    cache_hint: false,
                }],
            )
        };
        let body = serde_json::to_value(build_request(request)).expect("serialize");
        assert_eq!(body["system"][0]["text"], "be helpful");
        assert_eq!(body["inferenceConfig"]["maxTokens"], 1024);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(
            body["messages"][0]["content"][0]["toolResult"]["toolUseId"],
            "tu_1"
        );
        assert_eq!(
            body["messages"][0]["content"][0]["toolResult"]["content"][0]["text"],
            "done"
        );
        assert_eq!(
            body["messages"][0]["content"][0]["toolResult"]["status"],
            "success"
        );
    }

    #[test]
    fn omits_tool_config_without_tools() {
        let request = ChatRequest::new("m", vec![]);
        let body = serde_json::to_value(build_request(request)).expect("serialize");
        assert!(body.get("toolConfig").is_none());
    }

    #[test]
    fn enables_thinking_via_additional_model_request_fields() {
        let request = ChatRequest {
            temperature: Some(0.7),
            thinking: Some(agentloop_core::ThinkingConfig {
                budget_tokens: 8_192,
            }),
            ..ChatRequest::new("m", vec![])
        };
        let body = serde_json::to_value(build_request(request)).expect("serialize");
        assert_eq!(
            body["additionalModelRequestFields"]["thinking"]["type"],
            "enabled"
        );
        assert_eq!(
            body["additionalModelRequestFields"]["thinking"]["budget_tokens"],
            8192
        );
        // Thinking and temperature are mutually exclusive on Claude's Converse
        // API — the config must be dropped, not merely defaulted.
        assert!(body["inferenceConfig"].get("temperature").is_none());
    }

    #[test]
    fn omits_additional_model_request_fields_without_thinking() {
        let request = ChatRequest::new("m", vec![]);
        let body = serde_json::to_value(build_request(request)).expect("serialize");
        assert!(body.get("additionalModelRequestFields").is_none());
    }
}
