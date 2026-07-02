//! Private Anthropic Messages API wire types.

use std::collections::{BTreeMap, HashSet};

use agentloop_contracts::{
    BlobSource, ContentBlock, Message, MessageId, ModelInfo, Role, StopReason, TokenUsage,
    ToolCallId, ToolResultBlock,
};
use agentloop_core::{ChatRequest, ProviderStreamEvent, ToolChoice};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct AnthropicMessagesRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    stream: bool,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<AnthropicToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinking>,
}

#[derive(Debug, Serialize)]
struct AnthropicThinking {
    #[serde(rename = "type")]
    kind: &'static str,
    budget_tokens: u32,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Known(AnthropicKnownContent),
    Raw(serde_json::Value),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicKnownContent {
    Text {
        text: String,
    },
    Image {
        source: AnthropicBlobSource,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: Vec<AnthropicToolResultContent>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicBlobSource {
    Base64 { media_type: String, data: String },
    Url { url: String },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicToolResultContent {
    Text { text: String },
    Image { source: AnthropicBlobSource },
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicToolChoice {
    Auto,
    Any,
    Tool { name: String },
    None,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelList {
    pub data: Vec<ModelData>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub last_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelData {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    MessageStart {
        message: StreamMessage,
    },
    ContentBlockStart {
        index: usize,
        content_block: StreamContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: StreamDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: MessageDelta,
        usage: Option<Usage>,
    },
    MessageStop,
    Ping,
    Error {
        error: StreamError,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamContentBlock {
    Text {
        text: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Option<serde_json::Value>,
    },
    Thinking {
        thinking: Option<String>,
        signature: Option<String>,
    },
    RedactedThinking {
        data: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamDelta {
    TextDelta {
        text: String,
    },
    InputJsonDelta {
        partial_json: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    SignatureDelta {
        signature: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct MessageDelta {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct StreamError {
    message: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct AnthropicStreamMapper {
    requested_model: String,
    message_started: bool,
    ended: bool,
    last_output_tokens: u64,
    tool_ids: BTreeMap<usize, ToolCallId>,
    started_tools: HashSet<usize>,
}

impl AnthropicStreamMapper {
    pub(crate) fn new(requested_model: impl Into<String>) -> Self {
        Self {
            requested_model: requested_model.into(),
            message_started: false,
            ended: false,
            last_output_tokens: 0,
            tool_ids: BTreeMap::new(),
            started_tools: HashSet::new(),
        }
    }

    pub(crate) fn map_json(
        &mut self,
        data: &str,
    ) -> Result<Vec<ProviderStreamEvent>, serde_json::Error> {
        let event: StreamEvent = serde_json::from_str(data)?;
        let mut events = Vec::new();
        match event {
            StreamEvent::MessageStart { message } => {
                self.start_message(message.model, &mut events);
                if let Some(usage) = message.usage {
                    events.push(ProviderStreamEvent::Usage(usage_to_tokens(&usage, 0)));
                }
            }
            StreamEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                self.start_message(None, &mut events);
                self.map_content_block_start(index, content_block, &mut events);
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                self.map_delta(index, delta, &mut events);
            }
            StreamEvent::ContentBlockStop { index } => {
                if let Some(call_id) = self.tool_ids.get(&index) {
                    events.push(ProviderStreamEvent::ToolCallEnd {
                        call_id: call_id.clone(),
                    });
                }
            }
            StreamEvent::MessageDelta { delta, usage } => {
                if let Some(usage) = usage {
                    let current_output = usage.output_tokens;
                    let output_delta = current_output.saturating_sub(self.last_output_tokens);
                    self.last_output_tokens = current_output;
                    events.push(ProviderStreamEvent::Usage(usage_to_tokens(
                        &usage,
                        output_delta,
                    )));
                }
                if let Some(reason) = delta.stop_reason {
                    events.push(ProviderStreamEvent::MessageEnd {
                        stop_reason: stop_reason(&reason),
                    });
                    self.ended = true;
                }
            }
            StreamEvent::MessageStop => {
                if !self.ended {
                    events.push(ProviderStreamEvent::MessageEnd {
                        stop_reason: StopReason::EndTurn,
                    });
                    self.ended = true;
                }
            }
            StreamEvent::Error { error } => {
                events.push(ProviderStreamEvent::MarkdownDelta {
                    text: format!(
                        "\n\n[Anthropic stream error: {}]",
                        error.message.unwrap_or_else(|| "unknown".to_owned())
                    ),
                });
            }
            StreamEvent::Ping | StreamEvent::Unknown => {}
        }
        Ok(events)
    }

    pub(crate) fn ended(&self) -> bool {
        self.ended
    }

    fn start_message(&mut self, model: Option<String>, events: &mut Vec<ProviderStreamEvent>) {
        if self.message_started {
            return;
        }
        events.push(ProviderStreamEvent::MessageStart {
            message_id: MessageId::generate(),
            model: model.unwrap_or_else(|| self.requested_model.clone()),
        });
        self.message_started = true;
    }

    fn map_content_block_start(
        &mut self,
        index: usize,
        block: StreamContentBlock,
        events: &mut Vec<ProviderStreamEvent>,
    ) {
        match block {
            StreamContentBlock::Text { text } => {
                if let Some(text) = text.filter(|text| !text.is_empty()) {
                    events.push(ProviderStreamEvent::MarkdownDelta { text });
                }
            }
            StreamContentBlock::ToolUse { id, name, input } => {
                let call_id = ToolCallId::from(id);
                self.tool_ids.insert(index, call_id.clone());
                self.started_tools.insert(index);
                events.push(ProviderStreamEvent::ToolCallStart { call_id, name });
                if let Some(input) = input.filter(|input| input != &serde_json::Value::Null) {
                    if input != serde_json::json!({}) {
                        events.push(ProviderStreamEvent::ToolCallArgsDelta {
                            call_id: self.tool_ids[&index].clone(),
                            json_fragment: input.to_string(),
                        });
                    }
                }
            }
            StreamContentBlock::Thinking {
                thinking,
                signature,
            } => {
                if let Some(text) = thinking.filter(|text| !text.is_empty()) {
                    events.push(ProviderStreamEvent::ThinkingDelta { text });
                }
                if let Some(signature) = signature.filter(|value| !value.is_empty()) {
                    events.push(ProviderStreamEvent::ThinkingSignature { signature });
                }
            }
            StreamContentBlock::RedactedThinking { data } => {
                if let Some(data) = data.filter(|value| !value.is_empty()) {
                    events.push(ProviderStreamEvent::ThinkingSignature { signature: data });
                }
            }
            StreamContentBlock::Unknown => {}
        }
    }

    fn map_delta(
        &mut self,
        index: usize,
        delta: StreamDelta,
        events: &mut Vec<ProviderStreamEvent>,
    ) {
        match delta {
            StreamDelta::TextDelta { text } => {
                if !text.is_empty() {
                    events.push(ProviderStreamEvent::MarkdownDelta { text });
                }
            }
            StreamDelta::InputJsonDelta { partial_json } => {
                let call_id = self
                    .tool_ids
                    .entry(index)
                    .or_insert_with(ToolCallId::generate)
                    .clone();
                if !partial_json.is_empty() {
                    events.push(ProviderStreamEvent::ToolCallArgsDelta {
                        call_id,
                        json_fragment: partial_json,
                    });
                }
            }
            StreamDelta::ThinkingDelta { thinking } => {
                if !thinking.is_empty() {
                    events.push(ProviderStreamEvent::ThinkingDelta { text: thinking });
                }
            }
            StreamDelta::SignatureDelta { signature } => {
                if !signature.is_empty() {
                    events.push(ProviderStreamEvent::ThinkingSignature { signature });
                }
            }
            StreamDelta::Unknown => {}
        }
    }
}

pub(crate) fn build_request(request: ChatRequest) -> AnthropicMessagesRequest {
    AnthropicMessagesRequest {
        model: request.model,
        messages: build_messages(request.messages),
        stream: true,
        max_tokens: request.max_tokens.unwrap_or(4096),
        system: request.system.filter(|text| !text.trim().is_empty()),
        tools: request
            .tools
            .into_iter()
            .map(|tool| AnthropicTool {
                name: tool.name,
                description: tool.description,
                input_schema: tool.input_schema,
            })
            .collect(),
        tool_choice: tool_choice(request.tool_choice),
        temperature: request.temperature,
        thinking: request.thinking.map(|thinking| AnthropicThinking {
            kind: "enabled",
            budget_tokens: thinking.budget_tokens,
        }),
    }
}

pub(crate) fn models_from_response(response: ModelList) -> Vec<ModelInfo> {
    response.data.into_iter().map(model_data_to_info).collect()
}

fn model_data_to_info(model: ModelData) -> ModelInfo {
    ModelInfo {
        id: model.id,
        display_name: model.display_name,
        context_window: None,
        reasoning: false,
        vision: false,
    }
}

/// Merge paginated `/models` pages, preserving first-seen order and dropping
/// duplicate ids.
pub(crate) fn merge_model_pages(pages: impl IntoIterator<Item = Vec<ModelInfo>>) -> Vec<ModelInfo> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for page in pages {
        for model in page {
            if seen.insert(model.id.clone()) {
                out.push(model);
            }
        }
    }
    out
}

/// Append baseline Claude ids missing from an API listing so cheaper tiers
/// (Haiku) stay selectable when pagination or account visibility omits them.
pub(crate) fn supplement_known_models(mut models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    let present: std::collections::HashSet<String> = models.iter().map(|m| m.id.clone()).collect();
    for model in crate::config::known_anthropic_models() {
        if !present.contains(&model.id) {
            models.push(model);
        }
    }
    models
}

fn build_messages(messages: Vec<Message>) -> Vec<AnthropicMessage> {
    let mut out = Vec::new();
    for message in messages {
        let content = message
            .content
            .into_iter()
            .filter_map(content_block)
            .collect::<Vec<_>>();
        if !content.is_empty() {
            out.push(AnthropicMessage {
                role: role_name(message.role).to_owned(),
                content,
            });
        }
    }
    out
}

fn content_block(block: ContentBlock) -> Option<AnthropicContent> {
    let block = match block {
        ContentBlock::Markdown { text } => AnthropicKnownContent::Text { text },
        ContentBlock::Image { media_type, data } => {
            let source = blob_source(media_type, data)?;
            AnthropicKnownContent::Image { source }
        }
        ContentBlock::File {
            name,
            media_type,
            data,
        } => AnthropicKnownContent::Text {
            text: format!(
                "[file: {name}, {media_type}, {}]",
                render_blob_source(&data)
            ),
        },
        ContentBlock::Thinking { text, signature } => AnthropicKnownContent::Thinking {
            thinking: text,
            signature,
        },
        ContentBlock::ToolUse { id, name, input } => AnthropicKnownContent::ToolUse {
            id: id.to_string(),
            name,
            input,
        },
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => AnthropicKnownContent::ToolResult {
            tool_use_id: tool_use_id.to_string(),
            content: render_tool_result(content),
            is_error,
        },
        ContentBlock::Opaque { provider, data } => {
            if provider.as_str() == crate::config::ANTHROPIC_PROVIDER_ID {
                return Some(AnthropicContent::Raw(data));
            }
            return None;
        }
        _ => return None,
    };
    Some(AnthropicContent::Known(block))
}

fn render_tool_result(blocks: Vec<ToolResultBlock>) -> Vec<AnthropicToolResultContent> {
    blocks
        .into_iter()
        .filter_map(|block| match block {
            ToolResultBlock::Markdown { text } => Some(AnthropicToolResultContent::Text { text }),
            ToolResultBlock::Image { media_type, data } => {
                let source = blob_source(media_type, data)?;
                Some(AnthropicToolResultContent::Image { source })
            }
            ToolResultBlock::Json { value } => Some(AnthropicToolResultContent::Text {
                text: value.to_string(),
            }),
            _ => None,
        })
        .collect()
}

fn blob_source(media_type: String, data: BlobSource) -> Option<AnthropicBlobSource> {
    match data {
        BlobSource::Base64 { data } => Some(AnthropicBlobSource::Base64 { media_type, data }),
        BlobSource::Url { url } => Some(AnthropicBlobSource::Url { url }),
        BlobSource::Path { .. } => None,
        _ => None,
    }
}

fn render_blob_source(data: &BlobSource) -> String {
    match data {
        BlobSource::Base64 { .. } => "base64 data".to_owned(),
        BlobSource::Url { url } => format!("url {url}"),
        BlobSource::Path { path } => format!("path {}", path.display()),
        _ => "unknown source".to_owned(),
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "user",
        _ => "user",
    }
}

fn tool_choice(choice: ToolChoice) -> Option<AnthropicToolChoice> {
    match choice {
        ToolChoice::Auto => Some(AnthropicToolChoice::Auto),
        ToolChoice::None => Some(AnthropicToolChoice::None),
        ToolChoice::Required => Some(AnthropicToolChoice::Any),
        ToolChoice::Named(name) => Some(AnthropicToolChoice::Tool { name }),
    }
}

fn usage_to_tokens(usage: &Usage, output_tokens: u64) -> TokenUsage {
    TokenUsage {
        input: usage.input_tokens,
        output: output_tokens,
        cache_read: usage.cache_read_input_tokens,
        cache_write: usage.cache_creation_input_tokens,
        reasoning: None,
    }
}

fn stop_reason(reason: &str) -> StopReason {
    match reason {
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "refusal" => StopReason::Refusal,
        _ => StopReason::EndTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{ContentBlock, Message, ModelInfo};

    #[test]
    fn maps_anthropic_text_stream_to_provider_events() {
        let mut mapper = AnthropicStreamMapper::new("claude-test");
        let events = mapper.map_json(
            r#"{"type":"message_start","message":{"model":"claude-test","usage":{"input_tokens":3,"output_tokens":0}}}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(ProviderStreamEvent::MessageStart { .. })
                ));
                assert!(matches!(
                    events.get(1),
                    Some(ProviderStreamEvent::Usage(TokenUsage { input: 3, .. }))
                ));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }

        let events = mapper.map_json(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}"#,
        );
        match events {
            Ok(events) => assert!(matches!(
                events.first(),
                Some(ProviderStreamEvent::MarkdownDelta { text }) if text == "hello"
            )),
            Err(err) => panic!("stream chunk should parse: {err}"),
        }

        let events = mapper.map_json(
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":4}}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(ProviderStreamEvent::Usage(TokenUsage { output: 4, .. }))
                ));
                assert!(matches!(
                    events.get(1),
                    Some(ProviderStreamEvent::MessageEnd {
                        stop_reason: StopReason::EndTurn
                    })
                ));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }
    }

    #[test]
    fn maps_anthropic_tool_call_stream_to_provider_events() {
        let mut mapper = AnthropicStreamMapper::new("claude-test");
        let events = mapper.map_json(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}}"#,
        );
        match events {
            Ok(events) => {
                assert!(events.iter().any(|event| matches!(
                    event,
                    ProviderStreamEvent::ToolCallStart { call_id, name }
                        if call_id.as_str() == "toolu_1" && name == "Read"
                )));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }

        let events = mapper.map_json(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"README.md\"}"}}"#,
        );
        match events {
            Ok(events) => {
                assert!(events.iter().any(|event| matches!(
                    event,
                    ProviderStreamEvent::ToolCallArgsDelta { call_id, json_fragment }
                        if call_id.as_str() == "toolu_1" && json_fragment == "{\"file_path\":\"README.md\"}"
                )));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }
    }

    #[test]
    fn builds_tool_result_messages() {
        let request = ChatRequest::new(
            "claude-test",
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: ToolCallId::from("toolu_1"),
                    content: vec![ToolResultBlock::markdown("done")],
                    is_error: false,
                }],
                cache_hint: false,
            }],
        );
        let body = build_request(request);
        let json = serde_json::to_value(body);
        match json {
            Ok(value) => {
                assert_eq!(value["messages"][0]["role"], "user");
                assert_eq!(value["messages"][0]["content"][0]["type"], "tool_result");
                assert_eq!(value["messages"][0]["content"][0]["tool_use_id"], "toolu_1");
                assert_eq!(
                    value["messages"][0]["content"][0]["content"][0]["text"],
                    "done"
                );
            }
            Err(err) => panic!("request should serialize: {err}"),
        }
    }

    #[test]
    fn models_from_response_reads_pagination_metadata() {
        let page: ModelList = serde_json::from_str(
            r#"{
                "data":[{"id":"claude-opus-4-6","display_name":"Claude Opus 4.6"}],
                "has_more":true,
                "last_id":"claude-opus-4-6"
            }"#,
        )
        .expect("page json");
        assert!(page.has_more);
        assert_eq!(page.last_id.as_deref(), Some("claude-opus-4-6"));
        let models = models_from_response(page);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "claude-opus-4-6");
        assert_eq!(models[0].display_name.as_deref(), Some("Claude Opus 4.6"));
    }

    #[test]
    fn merge_model_pages_dedupes_across_pages() {
        let page_one = vec![ModelInfo {
            id: "claude-sonnet-4-5".to_owned(),
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        }];
        let page_two = vec![
            ModelInfo {
                id: "claude-sonnet-4-5".to_owned(),
                display_name: Some("duplicate".to_owned()),
                context_window: None,
                reasoning: false,
                vision: false,
            },
            ModelInfo {
                id: "claude-haiku-4-5".to_owned(),
                display_name: Some("Claude Haiku 4.5".to_owned()),
                context_window: None,
                reasoning: false,
                vision: false,
            },
        ];
        let merged = merge_model_pages([page_one, page_two]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "claude-sonnet-4-5");
        assert_eq!(merged[1].id, "claude-haiku-4-5");
    }

    #[test]
    fn supplement_known_models_includes_haiku_when_api_omits_it() {
        let api_only = vec![ModelInfo {
            id: "claude-sonnet-4-5".to_owned(),
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        }];
        let merged = supplement_known_models(api_only);
        assert!(
            merged.iter().any(|model| model.id == "claude-haiku-4-5"),
            "catalog should include haiku baseline: {:?}",
            merged.iter().map(|m| &m.id).collect::<Vec<_>>()
        );
    }
}
