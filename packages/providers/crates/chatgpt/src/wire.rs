//! Codex Responses API wire types (ChatGPT subscription backend).

use std::collections::HashMap;

use agentloop_contracts::{
    BlobSource, ContentBlock, Message, MessageId, Role, StopReason, TokenUsage, ToolCallId,
    ToolResultBlock,
};
use agentloop_core::{ChatRequest, ProviderStreamEvent, ToolChoice, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::resolve_model;

#[derive(Debug, Serialize)]
pub(crate) struct CodexResponsesRequest {
    model: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    instructions: String,
    input: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<CodexTool>,
    tool_choice: Value,
    parallel_tool_calls: bool,
    store: bool,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<CodexReasoning>,
    include: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CodexTool {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Serialize)]
struct CodexReasoning {
    effort: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsesEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    delta: Option<String>,
    #[serde(default)]
    item: Option<OutputItem>,
    #[serde(default)]
    response: Option<ResponsePayload>,
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutputItem {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResponsePayload {
    model: Option<String>,
    usage: Option<ResponsesUsage>,
    #[serde(default)]
    output: Vec<OutputItem>,
}

#[derive(Debug, Deserialize)]
struct ResponsesUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    output_tokens_details: Option<OutputTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct OutputTokenDetails {
    reasoning_tokens: Option<u64>,
}

#[derive(Debug, Default)]
pub(crate) struct CodexStreamMapper {
    requested_model: String,
    message_started: bool,
    /// Map Responses item id / call_id → ToolCallId.
    tool_ids: HashMap<String, ToolCallId>,
    ended: bool,
    saw_tool_use: bool,
}

impl CodexStreamMapper {
    pub(crate) fn new(requested_model: impl Into<String>) -> Self {
        Self {
            requested_model: requested_model.into(),
            message_started: false,
            tool_ids: HashMap::new(),
            ended: false,
            saw_tool_use: false,
        }
    }

    pub(crate) fn map_json(
        &mut self,
        data: &str,
    ) -> Result<Vec<ProviderStreamEvent>, serde_json::Error> {
        let event: ResponsesEvent = serde_json::from_str(data)?;
        let mut events = Vec::new();

        match event.kind.as_str() {
            "response.created" => {
                if !self.message_started {
                    let model = event
                        .response
                        .as_ref()
                        .and_then(|r| r.model.clone())
                        .unwrap_or_else(|| self.requested_model.clone());
                    events.push(ProviderStreamEvent::MessageStart {
                        message_id: MessageId::generate(),
                        model,
                    });
                    self.message_started = true;
                }
            }
            "response.output_text.delta" | "response.content_part.delta" => {
                self.ensure_started(&mut events);
                if let Some(text) = event.delta.filter(|t| !t.is_empty()) {
                    events.push(ProviderStreamEvent::MarkdownDelta { text });
                }
            }
            "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta"
            | "response.reasoning_summary_part.delta" => {
                self.ensure_started(&mut events);
                if let Some(text) = event.delta.filter(|t| !t.is_empty()) {
                    events.push(ProviderStreamEvent::ThinkingDelta { text });
                }
            }
            "response.output_item.added" => {
                self.ensure_started(&mut events);
                if let Some(item) = &event.item {
                    if item.kind.as_deref() == Some("function_call") {
                        self.saw_tool_use = true;
                        let call_id = self.track_tool(item);
                        let name = item.name.clone().unwrap_or_else(|| "unknown".to_owned());
                        events.push(ProviderStreamEvent::ToolCallStart { call_id, name });
                    }
                }
            }
            "response.function_call_arguments.delta" => {
                self.ensure_started(&mut events);
                let key = event
                    .call_id
                    .clone()
                    .or(event.item_id.clone())
                    .unwrap_or_default();
                if let Some(fragment) = event.arguments.or(event.delta) {
                    if !fragment.is_empty() {
                        let call_id = self
                            .tool_ids
                            .entry(key)
                            .or_insert_with(ToolCallId::generate)
                            .clone();
                        events.push(ProviderStreamEvent::ToolCallArgsDelta {
                            call_id,
                            json_fragment: fragment,
                        });
                    }
                }
            }
            "response.output_item.done" => {
                if let Some(item) = &event.item {
                    if item.kind.as_deref() == Some("function_call") {
                        self.saw_tool_use = true;
                        let call_id = self.track_tool(item);
                        if let Some(args) = item.arguments.as_ref().filter(|a| !a.is_empty()) {
                            // Some backends only emit full args on done.
                            if !self.tool_ids.contains_key(&call_id.to_string()) {
                                events.push(ProviderStreamEvent::ToolCallArgsDelta {
                                    call_id: call_id.clone(),
                                    json_fragment: args.clone(),
                                });
                            }
                        }
                        events.push(ProviderStreamEvent::ToolCallEnd { call_id });
                    }
                }
            }
            "response.completed" | "response.incomplete" | "response.failed" => {
                self.ensure_started(&mut events);
                if let Some(response) = &event.response {
                    if let Some(usage) = &response.usage {
                        events.push(ProviderStreamEvent::Usage(TokenUsage {
                            input: usage.input_tokens,
                            output: usage.output_tokens,
                            cache_read: None,
                            cache_write: None,
                            reasoning: usage
                                .output_tokens_details
                                .as_ref()
                                .and_then(|d| d.reasoning_tokens),
                        }));
                    }
                    // Scan output for any function calls we missed.
                    for item in &response.output {
                        if item.kind.as_deref() == Some("function_call") {
                            self.saw_tool_use = true;
                        }
                    }
                }
                if !self.ended {
                    let stop = if event.kind == "response.incomplete" {
                        StopReason::MaxTokens
                    } else if self.saw_tool_use {
                        StopReason::ToolUse
                    } else {
                        StopReason::EndTurn
                    };
                    events.push(ProviderStreamEvent::MessageEnd { stop_reason: stop });
                    self.ended = true;
                }
            }
            _ => {}
        }

        Ok(events)
    }

    fn ensure_started(&mut self, events: &mut Vec<ProviderStreamEvent>) {
        if !self.message_started {
            events.push(ProviderStreamEvent::MessageStart {
                message_id: MessageId::generate(),
                model: self.requested_model.clone(),
            });
            self.message_started = true;
        }
    }

    fn track_tool(&mut self, item: &OutputItem) -> ToolCallId {
        let key = item
            .call_id
            .clone()
            .or_else(|| item.id.clone())
            .unwrap_or_else(|| ToolCallId::generate().to_string());
        self.tool_ids
            .entry(key.clone())
            .or_insert_with(|| {
                item.call_id
                    .as_deref()
                    .or(item.id.as_deref())
                    .map(ToolCallId::from)
                    .unwrap_or_else(ToolCallId::generate)
            })
            .clone()
    }
}

pub(crate) fn build_request(request: ChatRequest) -> CodexResponsesRequest {
    let model = resolve_model(&request.model);
    let instructions = request.system.unwrap_or_default();
    let input = build_input(request.messages);
    let tools = request.tools.into_iter().map(map_tool).collect::<Vec<_>>();
    let tool_choice = map_tool_choice(request.tool_choice);
    let reasoning = request.thinking.map(|thinking| {
        let effort = if thinking.budget_tokens >= 16_000 {
            "high"
        } else if thinking.budget_tokens >= 4_000 {
            "medium"
        } else {
            "low"
        };
        CodexReasoning {
            effort: effort.to_owned(),
            summary: Some("auto".to_owned()),
        }
    });

    CodexResponsesRequest {
        model,
        instructions,
        input,
        tools,
        tool_choice,
        parallel_tool_calls: true,
        store: false,
        stream: true,
        reasoning,
        include: vec!["reasoning.encrypted_content".to_owned()],
    }
}

fn map_tool(tool: ToolSpec) -> CodexTool {
    CodexTool {
        kind: "function".to_owned(),
        name: tool.name,
        description: tool.description,
        parameters: tool.input_schema,
    }
}

fn map_tool_choice(choice: ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".to_owned()),
        ToolChoice::None => Value::String("none".to_owned()),
        ToolChoice::Required => Value::String("required".to_owned()),
        ToolChoice::Named(name) => serde_json::json!({
            "type": "function",
            "name": name,
        }),
    }
}

fn build_input(messages: Vec<Message>) -> Vec<Value> {
    let mut out = Vec::new();
    for message in messages {
        push_message(&mut out, message);
    }
    out
}

fn push_message(out: &mut Vec<Value>, message: Message) {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_results = Vec::new();

    for block in message.content {
        match block {
            ContentBlock::Markdown { text } => text_parts.push(text),
            ContentBlock::Thinking { .. } => {}
            ContentBlock::Image { media_type, data } => {
                text_parts.push(render_blob("image", &media_type, &data));
            }
            ContentBlock::File {
                name,
                media_type,
                data,
            } => {
                text_parts.push(format!(
                    "[file: {name}, {media_type}, {}]",
                    render_blob_source(&data)
                ));
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(serde_json::json!({
                    "type": "function_call",
                    "call_id": id.to_string(),
                    "name": name,
                    "arguments": input.to_string(),
                }));
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                tool_results.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": tool_use_id.to_string(),
                    "output": render_tool_result(&content),
                }));
            }
            ContentBlock::Opaque { .. } => {}
            _ => {}
        }
    }

    if !text_parts.is_empty() {
        let role = role_name(message.role);
        let content_type = if role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        out.push(serde_json::json!({
            "type": "message",
            "role": role,
            "content": [{
                "type": content_type,
                "text": text_parts.join("\n\n"),
            }],
        }));
    }
    out.extend(tool_calls);
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
        Role::System => "user",
        _ => "user",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{ContentBlock, Message, Role, ToolCallId};

    #[test]
    fn build_request_sets_codex_invariants() {
        let mut request = ChatRequest::new(
            "chatgpt/gpt-5.4",
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Markdown {
                    text: "hello".to_owned(),
                }],
                cache_hint: false,
            }],
        );
        request.system = Some("You are helpful.".to_owned());
        request.tools = vec![ToolSpec {
            name: "Read".to_owned(),
            description: "Read a file".to_owned(),
            input_schema: serde_json::json!({"type": "object"}),
        }];

        let body = build_request(request);
        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["model"], "gpt-5.4");
        assert_eq!(json["store"], false);
        assert_eq!(json["stream"], true);
        assert_eq!(json["instructions"], "You are helpful.");
        assert_eq!(json["tools"][0]["type"], "function");
        assert_eq!(json["tools"][0]["name"], "Read");
        assert!(json.get("max_output_tokens").is_none());
        assert_eq!(json["input"][0]["role"], "user");
    }

    #[test]
    fn maps_tool_round_trip_items() {
        let mut request = ChatRequest::new(
            "gpt-5.4",
            vec![
                Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::ToolUse {
                        id: ToolCallId::from("call_1"),
                        name: "Bash".to_owned(),
                        input: serde_json::json!({"command": "ls"}),
                    }],
                    cache_hint: false,
                },
                Message {
                    role: Role::User,
                    content: vec![ContentBlock::ToolResult {
                        tool_use_id: ToolCallId::from("call_1"),
                        content: vec![ToolResultBlock::Markdown {
                            text: "ok".to_owned(),
                        }],
                        is_error: false,
                    }],
                    cache_hint: false,
                },
            ],
        );
        request.system = Some("sys".to_owned());
        let body = build_request(request);
        let json = serde_json::to_value(&body).expect("serialize");
        assert_eq!(json["input"][0]["type"], "function_call");
        assert_eq!(json["input"][0]["call_id"], "call_1");
        assert_eq!(json["input"][1]["type"], "function_call_output");
        assert_eq!(json["input"][1]["output"], "ok");
    }

    #[test]
    fn maps_text_and_tool_sse_events() {
        let mut mapper = CodexStreamMapper::new("gpt-5.4");
        let created = r#"{"type":"response.created","response":{"model":"gpt-5.4"}}"#;
        let delta = r#"{"type":"response.output_text.delta","delta":"Hi"}"#;
        let tool_added = r#"{"type":"response.output_item.added","item":{"type":"function_call","call_id":"c1","name":"Read"}}"#;
        let args = r#"{"type":"response.function_call_arguments.delta","call_id":"c1","delta":"{\"path\""}"#;
        let done = r#"{"type":"response.output_item.done","item":{"type":"function_call","call_id":"c1","name":"Read","arguments":"{\"path\":\"a\"}"}}"#;
        let completed = r#"{"type":"response.completed","response":{"usage":{"input_tokens":10,"output_tokens":5},"output":[{"type":"function_call"}]}}"#;

        let e1 = mapper.map_json(created).expect("ok");
        assert!(matches!(e1[0], ProviderStreamEvent::MessageStart { .. }));
        let e2 = mapper.map_json(delta).expect("ok");
        assert_eq!(
            e2[0],
            ProviderStreamEvent::MarkdownDelta {
                text: "Hi".to_owned()
            }
        );
        let e3 = mapper.map_json(tool_added).expect("ok");
        assert!(matches!(
            e3[0],
            ProviderStreamEvent::ToolCallStart { ref name, .. } if name == "Read"
        ));
        let e4 = mapper.map_json(args).expect("ok");
        assert!(matches!(
            e4[0],
            ProviderStreamEvent::ToolCallArgsDelta { .. }
        ));
        let e5 = mapper.map_json(done).expect("ok");
        assert!(matches!(e5[0], ProviderStreamEvent::ToolCallEnd { .. }));
        let e6 = mapper.map_json(completed).expect("ok");
        assert!(e6.iter().any(|e| matches!(
            e,
            ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::ToolUse
            }
        )));
    }
}
