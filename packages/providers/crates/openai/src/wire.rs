//! Private OpenAI Chat Completions wire types.

use std::collections::{BTreeMap, HashSet};

use agentloop_contracts::{
    BlobSource, ContentBlock, Message, MessageId, ModelInfo, Role, StopReason, TokenUsage,
    ToolCallId, ToolResultBlock,
};
use agentloop_core::{ChatRequest, ProviderStreamEvent, ToolChoice};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct OpenAiChatRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    stream: bool,
    stream_options: StreamOptions,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<OpenAiToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<OpenAiThinking>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

/// Extended-thinking request config in the DeepSeek-style dialect:
/// `{"type":"enabled","budget_tokens":N}`. Only serialized when the caller
/// set [`ChatRequest::thinking`]; strict Chat Completions endpoints never
/// see the field.
#[derive(Debug, Serialize)]
struct OpenAiThinking {
    #[serde(rename = "type")]
    kind: String,
    budget_tokens: u32,
}

#[derive(Debug, Serialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    tool_calls: Vec<OpenAiToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiToolCallFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum OpenAiToolChoice {
    Mode(String),
    Named {
        #[serde(rename = "type")]
        kind: String,
        function: NamedFunction,
    },
}

#[derive(Debug, Serialize)]
struct NamedFunction {
    name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelList {
    pub data: Vec<ModelData>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelData {
    pub id: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    model: Option<String>,
    #[serde(default)]
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct Delta {
    content: Option<String>,
    /// DeepSeek dialect: reasoning text streamed alongside `content`.
    reasoning_content: Option<String>,
    /// Router/GLM variant of the same field.
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Vec<DeltaToolCall>,
}

#[derive(Debug, Deserialize)]
struct DeltaToolCall {
    index: usize,
    id: Option<String>,
    function: Option<DeltaFunction>,
}

#[derive(Debug, Deserialize)]
struct DeltaFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    completion_tokens_details: Option<CompletionDetails>,
}

#[derive(Debug, Deserialize)]
struct CompletionDetails {
    reasoning_tokens: Option<u64>,
}

#[derive(Debug, Default)]
pub(crate) struct OpenAiStreamMapper {
    requested_model: String,
    message_started: bool,
    tool_ids: BTreeMap<usize, ToolCallId>,
    started_tools: HashSet<usize>,
}

impl OpenAiStreamMapper {
    pub(crate) fn new(requested_model: impl Into<String>) -> Self {
        Self {
            requested_model: requested_model.into(),
            message_started: false,
            tool_ids: BTreeMap::new(),
            started_tools: HashSet::new(),
        }
    }

    pub(crate) fn map_json(
        &mut self,
        data: &str,
    ) -> Result<Vec<ProviderStreamEvent>, serde_json::Error> {
        let chunk: ChatCompletionChunk = serde_json::from_str(data)?;
        let mut events = Vec::new();
        if !self.message_started {
            events.push(ProviderStreamEvent::MessageStart {
                message_id: MessageId::generate(),
                model: chunk.model.unwrap_or_else(|| self.requested_model.clone()),
            });
            self.message_started = true;
        }

        if let Some(usage) = chunk.usage {
            events.push(ProviderStreamEvent::Usage(TokenUsage {
                input: usage.prompt_tokens,
                output: usage.completion_tokens,
                cache_read: None,
                cache_write: None,
                reasoning: usage
                    .completion_tokens_details
                    .and_then(|details| details.reasoning_tokens),
            }));
        }

        for choice in chunk.choices {
            if let Some(text) = choice.delta.reasoning_content.or(choice.delta.reasoning) {
                if !text.is_empty() {
                    events.push(ProviderStreamEvent::ThinkingDelta { text });
                }
            }

            if let Some(text) = choice.delta.content {
                if !text.is_empty() {
                    events.push(ProviderStreamEvent::MarkdownDelta { text });
                }
            }

            for tool_call in choice.delta.tool_calls {
                self.map_tool_call(tool_call, &mut events);
            }

            if let Some(reason) = choice.finish_reason {
                events.push(ProviderStreamEvent::MessageEnd {
                    stop_reason: stop_reason(&reason),
                });
            }
        }

        Ok(events)
    }

    fn map_tool_call(&mut self, tool_call: DeltaToolCall, events: &mut Vec<ProviderStreamEvent>) {
        let call_id = self
            .tool_ids
            .entry(tool_call.index)
            .or_insert_with(|| {
                tool_call
                    .id
                    .as_deref()
                    .map(ToolCallId::from)
                    .unwrap_or_else(ToolCallId::generate)
            })
            .clone();
        if let Some(function) = tool_call.function {
            if let Some(name) = function.name {
                if self.started_tools.insert(tool_call.index) {
                    events.push(ProviderStreamEvent::ToolCallStart {
                        call_id: call_id.clone(),
                        name,
                    });
                }
            }
            if let Some(arguments) = function.arguments {
                if !arguments.is_empty() {
                    events.push(ProviderStreamEvent::ToolCallArgsDelta {
                        call_id: call_id.clone(),
                        json_fragment: arguments,
                    });
                }
            }
        }
    }
}

pub(crate) fn build_request(request: ChatRequest) -> OpenAiChatRequest {
    let tools: Vec<OpenAiTool> = request
        .tools
        .into_iter()
        .map(|tool| OpenAiTool {
            kind: "function".to_owned(),
            function: OpenAiFunction {
                name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
            },
        })
        .collect();
    // Copilot (and some Chat Completions gateways) reject
    // `tool_choice` when `tools` is absent/empty — "tools are required when
    // tool choice is specified". Default `ChatRequest` uses Auto with no
    // tools (throwaway completions like session titles); omit the field.
    let tool_choice = if tools.is_empty() {
        None
    } else {
        tool_choice(request.tool_choice)
    };
    OpenAiChatRequest {
        model: request.model,
        messages: build_messages(request.system, request.messages),
        stream: true,
        stream_options: StreamOptions {
            include_usage: true,
        },
        tools,
        tool_choice,
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        thinking: request.thinking.map(|thinking| OpenAiThinking {
            kind: "enabled".to_owned(),
            budget_tokens: thinking.budget_tokens,
        }),
    }
}

pub(crate) fn models_from_response(response: ModelList) -> Vec<ModelInfo> {
    response
        .data
        .into_iter()
        .map(|model| ModelInfo {
            id: model.id,
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        })
        .collect()
}

fn build_messages(system: Option<String>, messages: Vec<Message>) -> Vec<OpenAiMessage> {
    let mut out = Vec::new();
    if let Some(system) = system.filter(|text| !text.trim().is_empty()) {
        out.push(OpenAiMessage {
            role: "system".to_owned(),
            content: Some(system),
            tool_calls: Vec::new(),
            tool_call_id: None,
        });
    }
    for message in messages {
        push_message(&mut out, message);
    }
    out
}

fn push_message(out: &mut Vec<OpenAiMessage>, message: Message) {
    let mut text = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();

    for block in message.content {
        match block {
            ContentBlock::Markdown { text: value } => {
                text.push(value);
            }
            ContentBlock::Thinking { .. } => {}
            ContentBlock::Image { media_type, data } => {
                text.push(render_blob("image", &media_type, &data));
            }
            ContentBlock::File {
                name,
                media_type,
                data,
            } => {
                text.push(format!(
                    "[file: {name}, {media_type}, {}]",
                    render_blob_source(&data)
                ));
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(OpenAiToolCall {
                    id: id.to_string(),
                    kind: "function".to_owned(),
                    function: OpenAiToolCallFunction {
                        name,
                        arguments: input.to_string(),
                    },
                });
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                tool_results.push(OpenAiMessage {
                    role: "tool".to_owned(),
                    content: Some(render_tool_result(&content)),
                    tool_calls: Vec::new(),
                    tool_call_id: Some(tool_use_id.to_string()),
                });
            }
            ContentBlock::Opaque { .. } => {}
            _ => {}
        }
    }

    if !text.is_empty() || !tool_calls.is_empty() {
        out.push(OpenAiMessage {
            role: role_name(message.role).to_owned(),
            content: (!text.is_empty()).then(|| text.join("\n\n")),
            tool_calls,
            tool_call_id: None,
        });
    }
    out.extend(tool_results);
}

fn render_tool_result(blocks: &[ToolResultBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            ToolResultBlock::Markdown { text } => text.clone(),
            ToolResultBlock::Image { media_type, data } => render_blob("image", media_type, data),
            ToolResultBlock::Json { value } => value.to_string(),
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_blob(kind: &str, media_type: &str, data: &BlobSource) -> String {
    format!("[{kind}: {media_type}, {}]", render_blob_source(data))
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
        Role::System => "system",
        _ => "user",
    }
}

fn tool_choice(choice: ToolChoice) -> Option<OpenAiToolChoice> {
    match choice {
        ToolChoice::Auto => Some(OpenAiToolChoice::Mode("auto".to_owned())),
        ToolChoice::None => Some(OpenAiToolChoice::Mode("none".to_owned())),
        ToolChoice::Required => Some(OpenAiToolChoice::Mode("required".to_owned())),
        ToolChoice::Named(name) => Some(OpenAiToolChoice::Named {
            kind: "function".to_owned(),
            function: NamedFunction { name },
        }),
    }
}

fn stop_reason(reason: &str) -> StopReason {
    match reason {
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        "content_filter" => StopReason::Refusal,
        _ => StopReason::EndTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{ContentBlock, Message};

    #[test]
    fn maps_openai_text_stream_to_provider_events() {
        let mut mapper = OpenAiStreamMapper::new("gpt-test");
        let events = mapper.map_json(
            r#"{"model":"gpt-test","choices":[{"delta":{"role":"assistant","content":"hello"},"finish_reason":null}]}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(ProviderStreamEvent::MessageStart { .. })
                ));
                assert!(matches!(
                    events.get(1),
                    Some(ProviderStreamEvent::MarkdownDelta { text }) if text == "hello"
                ));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }

        let events = mapper.map_json(
            r#"{"model":"gpt-test","choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":4,"completion_tokens_details":{"reasoning_tokens":1}}}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(ProviderStreamEvent::Usage(TokenUsage {
                        input: 3,
                        output: 4,
                        reasoning: Some(1),
                        ..
                    }))
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
    fn maps_openai_tool_call_stream_to_provider_events() {
        let mut mapper = OpenAiStreamMapper::new("gpt-test");
        let events = mapper.map_json(
            r#"{"model":"gpt-test","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"Read","arguments":"{\"file_path\""}}]},"finish_reason":null}]}"#,
        );
        match events {
            Ok(events) => {
                assert!(events.iter().any(|event| matches!(
                    event,
                    ProviderStreamEvent::ToolCallStart { call_id, name }
                        if call_id.as_str() == "call_1" && name == "Read"
                )));
                assert!(events.iter().any(|event| matches!(
                    event,
                    ProviderStreamEvent::ToolCallArgsDelta { call_id, json_fragment }
                        if call_id.as_str() == "call_1" && json_fragment == "{\"file_path\""
                )));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }
    }

    #[test]
    fn maps_reasoning_content_delta_to_thinking_events() {
        let mut mapper = OpenAiStreamMapper::new("deepseek-reasoner");
        let events = mapper.map_json(
            r#"{"model":"deepseek-reasoner","choices":[{"delta":{"reasoning_content":"pondering"},"finish_reason":null}]}"#,
        );
        match events {
            Ok(events) => assert!(matches!(
                events.get(1),
                Some(ProviderStreamEvent::ThinkingDelta { text }) if text == "pondering"
            )),
            Err(err) => panic!("stream chunk should parse: {err}"),
        }
    }

    #[test]
    fn maps_reasoning_delta_variant_to_thinking_events() {
        let mut mapper = OpenAiStreamMapper::new("glm-test");
        let events = mapper.map_json(
            r#"{"model":"glm-test","choices":[{"delta":{"reasoning":"weighing options","content":"answer"},"finish_reason":null}]}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.get(1),
                    Some(ProviderStreamEvent::ThinkingDelta { text }) if text == "weighing options"
                ));
                assert!(matches!(
                    events.get(2),
                    Some(ProviderStreamEvent::MarkdownDelta { text }) if text == "answer"
                ));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }
    }

    #[test]
    fn omits_tool_choice_when_tools_are_empty() {
        // Default ChatRequest is Auto + no tools — must not serialize
        // tool_choice (Copilot: "tools are required when tool choice is
        // specified").
        let bare = ChatRequest::new("gpt-test", Vec::new());
        let json = match serde_json::to_value(build_request(bare)) {
            Ok(value) => value,
            Err(err) => panic!("request should serialize: {err}"),
        };
        assert!(
            json.get("tools").is_none(),
            "empty tools must be omitted: {json}"
        );
        assert!(
            json.get("tool_choice").is_none(),
            "tool_choice must be omitted without tools: {json}"
        );

        let mut with_tools = ChatRequest::new("gpt-test", Vec::new());
        with_tools.tools.push(agentloop_core::ToolSpec {
            name: "Read".to_owned(),
            description: "read a file".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
        });
        let json = match serde_json::to_value(build_request(with_tools)) {
            Ok(value) => value,
            Err(err) => panic!("request should serialize: {err}"),
        };
        assert_eq!(json["tool_choice"], "auto");
        assert_eq!(json["tools"][0]["function"]["name"], "Read");
    }

    #[test]
    fn serializes_thinking_config_and_omits_it_when_unset() {
        let mut request = ChatRequest::new("deepseek-chat", Vec::new());
        request.thinking = Some(agentloop_core::ThinkingConfig {
            budget_tokens: 2048,
        });
        let json = match serde_json::to_value(build_request(request)) {
            Ok(value) => value,
            Err(err) => panic!("request should serialize: {err}"),
        };
        assert_eq!(json["thinking"]["type"], "enabled");
        assert_eq!(json["thinking"]["budget_tokens"], 2048);

        let bare = ChatRequest::new("deepseek-chat", Vec::new());
        let json = match serde_json::to_value(build_request(bare)) {
            Ok(value) => value,
            Err(err) => panic!("request should serialize: {err}"),
        };
        assert!(
            json.get("thinking").is_none(),
            "unset thinking must not reach the wire: {json}"
        );
    }

    #[test]
    fn replay_skips_thinking_blocks() {
        let request = ChatRequest::new(
            "deepseek-chat",
            vec![Message {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Thinking {
                        text: "private reasoning".to_owned(),
                        signature: None,
                    },
                    ContentBlock::markdown("visible answer"),
                ],
                cache_hint: false,
            }],
        );
        let json = match serde_json::to_value(build_request(request)) {
            Ok(value) => value,
            Err(err) => panic!("request should serialize: {err}"),
        };
        assert_eq!(json["messages"][0]["content"], "visible answer");
    }

    #[test]
    fn builds_tool_result_messages() {
        let request = ChatRequest::new(
            "gpt-test",
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: ToolCallId::from("call_1"),
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
                assert_eq!(value["messages"][0]["role"], "tool");
                assert_eq!(value["messages"][0]["tool_call_id"], "call_1");
                assert_eq!(value["messages"][0]["content"], "done");
            }
            Err(err) => panic!("request should serialize: {err}"),
        }
    }
}
