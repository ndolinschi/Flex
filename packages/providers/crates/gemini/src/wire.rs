use std::collections::BTreeMap;

use agentloop_contracts::{
    BlobSource, ContentBlock, Message, MessageId, ModelInfo, Role, StopReason, TokenUsage,
    ToolCallId, ToolResultBlock,
};
use agentloop_core::{ChatRequest, ProviderStreamEvent, ToolChoice};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeminiGenerateContentRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<GeminiToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inline_data: Option<GeminiInlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_data: Option<GeminiFileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiInlineData {
    mime_type: String,
    data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiFileData {
    mime_type: String,
    file_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    #[serde(default)]
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiToolConfig {
    function_calling_config: GeminiFunctionCallingConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiFunctionCallingConfig {
    mode: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    allowed_function_names: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelList {
    pub models: Vec<ModelData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelData {
    pub name: String,
    pub display_name: Option<String>,
    pub input_token_limit: Option<u64>,
    #[serde(default)]
    pub supported_generation_methods: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateContentResponse {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsage {
    #[serde(default)]
    prompt_token_count: u64,
    #[serde(default)]
    candidates_token_count: u64,
    thoughts_token_count: Option<u64>,
}

#[derive(Debug, Default)]
pub(crate) struct GeminiStreamMapper {
    requested_model: String,
    message_started: bool,
    ended: bool,
}

impl GeminiStreamMapper {
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
        let chunk: GenerateContentResponse = serde_json::from_str(data)?;
        let mut events = Vec::new();
        self.start_message(&mut events);

        if let Some(usage) = chunk.usage_metadata {
            events.push(ProviderStreamEvent::Usage(TokenUsage {
                input: usage.prompt_token_count,
                output: usage.candidates_token_count,
                cache_read: None,
                cache_write: None,
                reasoning: usage.thoughts_token_count,
            }));
        }

        for candidate in chunk.candidates {
            if let Some(content) = candidate.content {
                for part in content.parts {
                    self.map_part(part, &mut events);
                }
            }
            if let Some(reason) = candidate.finish_reason {
                events.push(ProviderStreamEvent::MessageEnd {
                    stop_reason: stop_reason(&reason),
                });
                self.ended = true;
            }
        }

        Ok(events)
    }

    pub(crate) fn ended(&self) -> bool {
        self.ended
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

    fn map_part(&mut self, part: GeminiPart, events: &mut Vec<ProviderStreamEvent>) {
        if let Some(text) = part.text.filter(|text| !text.is_empty()) {
            events.push(ProviderStreamEvent::MarkdownDelta { text });
        }
        if let Some(function_call) = part.function_call {
            let call_id = ToolCallId::generate();
            events.push(ProviderStreamEvent::ToolCallStart {
                call_id: call_id.clone(),
                name: function_call.name,
            });
            if function_call.args != serde_json::Value::Null {
                events.push(ProviderStreamEvent::ToolCallArgsDelta {
                    call_id: call_id.clone(),
                    json_fragment: function_call.args.to_string(),
                });
            }
            events.push(ProviderStreamEvent::ToolCallEnd { call_id });
        }
    }
}

pub(crate) fn build_request(request: ChatRequest) -> GeminiGenerateContentRequest {
    GeminiGenerateContentRequest {
        contents: build_messages(request.messages),
        system_instruction: request
            .system
            .filter(|text| !text.trim().is_empty())
            .map(|text| GeminiContent {
                role: None,
                parts: vec![GeminiPart::text(text)],
            }),
        tools: tools(request.tools),
        tool_config: tool_choice(request.tool_choice),
        generation_config: generation_config(request.max_tokens, request.temperature),
    }
}

pub(crate) fn models_from_response(response: ModelList) -> Vec<ModelInfo> {
    response
        .models
        .into_iter()
        .map(|model| ModelInfo {
            id: model
                .name
                .strip_prefix("models/")
                .unwrap_or(&model.name)
                .to_owned(),
            display_name: model.display_name,
            context_window: model
                .input_token_limit
                .and_then(|value| u32::try_from(value).ok()),
            reasoning: false,
            vision: model
                .supported_generation_methods
                .iter()
                .any(|method| method == "generateContent" || method == "streamGenerateContent"),
        })
        .collect()
}

impl GeminiPart {
    fn text(text: String) -> Self {
        Self {
            text: Some(text),
            inline_data: None,
            file_data: None,
            function_call: None,
            function_response: None,
        }
    }

    fn inline_data(mime_type: String, data: String) -> Self {
        Self {
            text: None,
            inline_data: Some(GeminiInlineData { mime_type, data }),
            file_data: None,
            function_call: None,
            function_response: None,
        }
    }

    fn file_data(mime_type: String, file_uri: String) -> Self {
        Self {
            text: None,
            inline_data: None,
            file_data: Some(GeminiFileData {
                mime_type,
                file_uri,
            }),
            function_call: None,
            function_response: None,
        }
    }

    fn function_response(name: String, response: serde_json::Value) -> Self {
        Self {
            text: None,
            inline_data: None,
            file_data: None,
            function_call: None,
            function_response: Some(GeminiFunctionResponse { name, response }),
        }
    }
}

fn build_messages(messages: Vec<Message>) -> Vec<GeminiContent> {
    let mut tool_names = BTreeMap::new();
    messages
        .into_iter()
        .filter_map(|message| {
            let role = role_name(message.role).to_owned();
            let parts = message
                .content
                .into_iter()
                .filter_map(|block| content_block(block, &mut tool_names))
                .collect::<Vec<_>>();
            (!parts.is_empty()).then_some(GeminiContent {
                role: Some(role),
                parts,
            })
        })
        .collect()
}

fn content_block(
    block: ContentBlock,
    tool_names: &mut BTreeMap<ToolCallId, String>,
) -> Option<GeminiPart> {
    match block {
        ContentBlock::Markdown { text } | ContentBlock::Thinking { text, .. } => {
            Some(GeminiPart::text(text))
        }
        ContentBlock::Image { media_type, data } => blob_part(media_type, data),
        ContentBlock::File {
            name,
            media_type,
            data,
        } => Some(GeminiPart::text(format!(
            "[file: {name}, {media_type}, {}]",
            render_blob_source(&data)
        ))),
        ContentBlock::ToolUse { id, name, input } => {
            tool_names.insert(id, name.clone());
            Some(GeminiPart {
                text: None,
                inline_data: None,
                file_data: None,
                function_call: Some(GeminiFunctionCall { name, args: input }),
                function_response: None,
            })
        }
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            ..
        } => {
            let name = tool_names
                .get(&tool_use_id)
                .cloned()
                .unwrap_or_else(|| tool_use_id.to_string());
            Some(GeminiPart::function_response(
                name,
                serde_json::json!({ "content": render_tool_result(content) }),
            ))
        }
        _ => None,
    }
}

fn blob_part(media_type: String, data: BlobSource) -> Option<GeminiPart> {
    match data {
        BlobSource::Base64 { data } => Some(GeminiPart::inline_data(media_type, data)),
        BlobSource::Url { url } => Some(GeminiPart::file_data(media_type, url)),
        BlobSource::Path { path } => Some(GeminiPart::text(format!(
            "[image: {media_type}, path {}]",
            path.display()
        ))),
        _ => None,
    }
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

fn render_blob_source(data: &BlobSource) -> String {
    match data {
        BlobSource::Base64 { .. } => "base64 data".to_owned(),
        BlobSource::Url { url } => format!("url {url}"),
        BlobSource::Path { path } => format!("path {}", path.display()),
        _ => "unknown source".to_owned(),
    }
}

fn tools(tools: Vec<agentloop_core::ToolSpec>) -> Vec<GeminiTool> {
    if tools.is_empty() {
        return Vec::new();
    }
    vec![GeminiTool {
        function_declarations: tools
            .into_iter()
            .map(|tool| GeminiFunctionDeclaration {
                name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
            })
            .collect(),
    }]
}

fn tool_choice(choice: ToolChoice) -> Option<GeminiToolConfig> {
    let (mode, allowed_function_names) = match choice {
        ToolChoice::Auto => ("AUTO".to_owned(), Vec::new()),
        ToolChoice::None => ("NONE".to_owned(), Vec::new()),
        ToolChoice::Required => ("ANY".to_owned(), Vec::new()),
        ToolChoice::Named(name) => ("ANY".to_owned(), vec![name]),
    };
    Some(GeminiToolConfig {
        function_calling_config: GeminiFunctionCallingConfig {
            mode,
            allowed_function_names,
        },
    })
}

fn generation_config(
    max_output_tokens: Option<u32>,
    temperature: Option<f32>,
) -> Option<GeminiGenerationConfig> {
    (max_output_tokens.is_some() || temperature.is_some()).then_some(GeminiGenerationConfig {
        max_output_tokens,
        temperature,
    })
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Assistant => "model",
        Role::System | Role::User => "user",
        _ => "user",
    }
}

fn stop_reason(reason: &str) -> StopReason {
    match reason {
        "MAX_TOKENS" => StopReason::MaxTokens,
        "SAFETY" | "RECITATION" => StopReason::Refusal,
        _ => StopReason::EndTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{ContentBlock, Message};

    #[test]
    fn maps_gemini_text_stream_to_provider_events() {
        let mut mapper = GeminiStreamMapper::new("gemini-test");
        let events = mapper.map_json(
            r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"hello"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":4,"thoughtsTokenCount":1}}"#,
        );
        match events {
            Ok(events) => {
                assert!(matches!(
                    events.first(),
                    Some(ProviderStreamEvent::MessageStart { .. })
                ));
                assert!(matches!(
                    events.get(1),
                    Some(ProviderStreamEvent::Usage(TokenUsage {
                        input: 3,
                        output: 4,
                        reasoning: Some(1),
                        ..
                    }))
                ));
                assert!(matches!(
                    events.get(2),
                    Some(ProviderStreamEvent::MarkdownDelta { text }) if text == "hello"
                ));
                assert!(matches!(
                    events.get(3),
                    Some(ProviderStreamEvent::MessageEnd {
                        stop_reason: StopReason::EndTurn
                    })
                ));
            }
            Err(err) => panic!("stream chunk should parse: {err}"),
        }
    }

    #[test]
    fn maps_gemini_function_call_to_tool_events() {
        let mut mapper = GeminiStreamMapper::new("gemini-test");
        let events = mapper.map_json(
            r#"{"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"Read","args":{"file_path":"README.md"}}}]}}]}"#,
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
            Err(err) => panic!("stream chunk should parse: {err}"),
        }
    }

    #[test]
    fn builds_tool_result_parts() {
        let request = ChatRequest::new(
            "gemini-test",
            vec![
                Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::ToolUse {
                        id: ToolCallId::from("call_1"),
                        name: "Read".to_owned(),
                        input: serde_json::json!({"file_path":"README.md"}),
                    }],
                    cache_hint: false,
                },
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::ToolResult {
                        tool_use_id: ToolCallId::from("call_1"),
                        content: vec![ToolResultBlock::markdown("done")],
                        is_error: false,
                    }],
                    cache_hint: false,
                },
            ],
        );
        let body = build_request(request);
        let json = serde_json::to_value(body);
        match json {
            Ok(value) => {
                assert_eq!(
                    value["contents"][1]["parts"][0]["functionResponse"]["name"],
                    "Read"
                );
                assert_eq!(
                    value["contents"][1]["parts"][0]["functionResponse"]["response"]["content"],
                    "done"
                );
            }
            Err(err) => panic!("request should serialize: {err}"),
        }
    }
}
