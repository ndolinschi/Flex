use agentloop_contracts::{
    BlobSource, ContentBlock, Message, MessageId, ModelInfo, Role, StopReason, TokenUsage,
    ToolCallId, ToolResultBlock,
};
use agentloop_core::{ChatRequest, ProviderStreamEvent, ToolChoice};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OllamaTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OllamaToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Debug, Serialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    kind: String,
    function: OllamaFunction,
}

#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelList {
    pub models: Vec<ModelData>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelData {
    pub name: String,
}

#[derive(Debug, Deserialize)]
struct OllamaChatChunk {
    model: Option<String>,
    message: Option<OllamaResponseMessage>,
    #[serde(default)]
    done: bool,
    done_reason: Option<String>,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OllamaResponseToolCall>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseToolCall {
    function: OllamaResponseFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseFunction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

#[derive(Debug, Default)]
pub(crate) struct OllamaStreamMapper {
    requested_model: String,
    message_started: bool,
    ended: bool,
}

impl OllamaStreamMapper {
    pub(crate) fn new(requested_model: impl Into<String>) -> Self {
        Self {
            requested_model: requested_model.into(),
            message_started: false,
            ended: false,
        }
    }

    pub(crate) fn map_json(
        &mut self,
        data: &str,
    ) -> Result<Vec<ProviderStreamEvent>, serde_json::Error> {
        let chunk: OllamaChatChunk = serde_json::from_str(data)?;
        let mut events = Vec::new();
        self.start_message(chunk.model, &mut events);

        if let Some(message) = chunk.message {
            if let Some(text) = message.content.filter(|text| !text.is_empty()) {
                events.push(ProviderStreamEvent::MarkdownDelta { text });
            }
            for tool_call in message.tool_calls {
                let call_id = ToolCallId::generate();
                events.push(ProviderStreamEvent::ToolCallStart {
                    call_id: call_id.clone(),
                    name: tool_call.function.name,
                });
                if tool_call.function.arguments != serde_json::Value::Null {
                    events.push(ProviderStreamEvent::ToolCallArgsDelta {
                        call_id: call_id.clone(),
                        json_fragment: tool_call.function.arguments.to_string(),
                    });
                }
                events.push(ProviderStreamEvent::ToolCallEnd { call_id });
            }
        }

        if chunk.done {
            if chunk.prompt_eval_count.is_some() || chunk.eval_count.is_some() {
                events.push(ProviderStreamEvent::Usage(TokenUsage {
                    input: chunk.prompt_eval_count.unwrap_or(0),
                    output: chunk.eval_count.unwrap_or(0),
                    cache_read: None,
                    cache_write: None,
                    reasoning: None,
                }));
            }
            events.push(ProviderStreamEvent::MessageEnd {
                stop_reason: stop_reason(chunk.done_reason.as_deref()),
            });
            self.ended = true;
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
}

pub(crate) fn build_request(request: ChatRequest) -> OllamaChatRequest {
    let mut messages = build_messages(request.messages);
    if let Some(system) = request.system.filter(|text| !text.trim().is_empty()) {
        messages.insert(
            0,
            OllamaMessage {
                role: "system".to_owned(),
                content: system,
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        );
    }

    OllamaChatRequest {
        model: request.model,
        messages,
        stream: true,
        tools: tools(request.tools, request.tool_choice),
        options: options(request.max_tokens, request.temperature),
    }
}

pub(crate) fn models_from_response(response: ModelList) -> Vec<ModelInfo> {
    response
        .models
        .into_iter()
        .map(|model| ModelInfo {
            id: model.name,
            display_name: None,
            context_window: None,
            reasoning: false,
            vision: false,
        })
        .collect()
}

fn build_messages(messages: Vec<Message>) -> Vec<OllamaMessage> {
    let mut out = Vec::new();
    for message in messages {
        let mut text = Vec::new();
        let mut tool_calls = Vec::new();
        let mut tool_results = Vec::new();
        for block in message.content {
            match block {
                ContentBlock::Markdown { text: value }
                | ContentBlock::Thinking { text: value, .. } => text.push(value),
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
                ContentBlock::ToolUse { name, input, .. } => {
                    tool_calls.push(OllamaToolCall {
                        function: OllamaToolCallFunction {
                            name,
                            arguments: input,
                        },
                    });
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } => {
                    tool_results.push(OllamaMessage {
                        role: "tool".to_owned(),
                        content: render_tool_result(content),
                        tool_calls: Vec::new(),
                        tool_call_id: Some(tool_use_id.to_string()),
                    });
                }
                _ => {}
            }
        }
        if !text.is_empty() || !tool_calls.is_empty() {
            out.push(OllamaMessage {
                role: role_name(message.role).to_owned(),
                content: text.join("\n\n"),
                tool_calls,
                tool_call_id: None,
            });
        }
        out.extend(tool_results);
    }
    out
}

fn render_tool_result(blocks: Vec<ToolResultBlock>) -> String {
    blocks
        .into_iter()
        .map(|block| match block {
            ToolResultBlock::Markdown { text } => text,
            ToolResultBlock::Json { value } => value.to_string(),
            ToolResultBlock::Image { media_type, data } => {
                format!("[image: {media_type}, {}]", render_blob_source(&data))
            }
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

fn tools(tools: Vec<agentloop_core::ToolSpec>, tool_choice: ToolChoice) -> Vec<OllamaTool> {
    if matches!(tool_choice, ToolChoice::None) {
        return Vec::new();
    }
    tools
        .into_iter()
        .filter(|tool| match &tool_choice {
            ToolChoice::Named(name) => &tool.name == name,
            _ => true,
        })
        .map(|tool| OllamaTool {
            kind: "function".to_owned(),
            function: OllamaFunction {
                name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
            },
        })
        .collect()
}

fn options(num_predict: Option<u32>, temperature: Option<f32>) -> Option<OllamaOptions> {
    (num_predict.is_some() || temperature.is_some()).then_some(OllamaOptions {
        num_predict,
        temperature,
    })
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::User => "user",
        _ => "user",
    }
}

fn stop_reason(reason: Option<&str>) -> StopReason {
    match reason {
        Some("length") => StopReason::MaxTokens,
        _ => StopReason::EndTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{ContentBlock, Message};

    #[test]
    fn maps_ollama_text_stream_to_provider_events() {
        let mut mapper = OllamaStreamMapper::new("llama-test");
        let events = mapper.map_json(
            r#"{"model":"llama-test","message":{"role":"assistant","content":"hello"},"done":false}"#,
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
            Err(err) => panic!("stream line should parse: {err}"),
        }

        let events = mapper
            .map_json(r#"{"done":true,"done_reason":"stop","prompt_eval_count":3,"eval_count":4}"#);
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(ProviderStreamEvent::Usage(TokenUsage {
                        input: 3,
                        output: 4,
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
            Err(err) => panic!("stream line should parse: {err}"),
        }
    }

    #[test]
    fn maps_ollama_tool_call_stream_to_provider_events() {
        let mut mapper = OllamaStreamMapper::new("llama-test");
        let events = mapper.map_json(
            r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"Read","arguments":{"file_path":"README.md"}}}]},"done":false}"#,
        );
        match events {
            Ok(events) => {
                assert!(events.iter().any(|event| matches!(
                    event,
                    ProviderStreamEvent::ToolCallStart { name, .. } if name == "Read"
                )));
                assert!(events.iter().any(|event| matches!(
                    event,
                    ProviderStreamEvent::ToolCallArgsDelta { json_fragment, .. }
                        if json_fragment == "{\"file_path\":\"README.md\"}"
                )));
                assert!(
                    events
                        .iter()
                        .any(|event| matches!(event, ProviderStreamEvent::ToolCallEnd { .. }))
                );
            }
            Err(err) => panic!("stream line should parse: {err}"),
        }
    }

    #[test]
    fn builds_tool_result_messages() {
        let request = ChatRequest::new(
            "llama-test",
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
