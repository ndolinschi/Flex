//! `WebFetch`: fetch a URL as text.

use async_trait::async_trait;
use reqwest::Client;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::{schema_of, truncate_chars};

const DEFAULT_MAX_BYTES: usize = 200_000;
const HARD_MAX_BYTES: usize = 1_000_000;
const MAX_OUTPUT_CHARS: usize = 120_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WebFetchInput {
    /// HTTP or HTTPS URL to fetch.
    url: String,
    /// Maximum response bytes to keep. Defaults to 200000, capped at 1000000.
    max_bytes: Option<usize>,
}

/// Fetch HTTP(S) content and return token-efficient text.
#[derive(Clone)]
pub struct WebFetchTool {
    client: Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "WebFetch".to_owned(),
            description: "Fetch an HTTP(S) URL and return its text body with status and content \
                          type metadata. Use `max_bytes` for large pages. Non-success HTTP \
                          statuses are returned as tool errors so the model can decide whether \
                          to retry with a different URL."
                .to_owned(),
            input_schema: schema_of::<WebFetchInput>(),
            read_only: true,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: WebFetchInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `WebFetch` must be {{\"url\": \"https://...\", \"max_bytes\": \
                 <optional number>}}: {err}."
            ))
        })?;
        let url = reqwest::Url::parse(&input.url).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`url` is not a valid absolute URL for WebFetch: {err}. Pass an http:// or \
                 https:// URL."
            ))
        })?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(ToolError::InvalidInput(format!(
                "WebFetch supports only http:// and https:// URLs, but got `{}`.",
                url.scheme()
            )));
        }
        let max_bytes = input
            .max_bytes
            .unwrap_or(DEFAULT_MAX_BYTES)
            .min(HARD_MAX_BYTES);

        let response = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = self.client.get(url.clone()).send() => result.map_err(|err| {
                ToolError::Execution(format!("WebFetch request to `{url}` failed: {err}."))
            })?,
        };
        let status = response.status();
        let final_url = response.url().clone();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        if !status.is_success() {
            return Err(ToolError::Execution(format!(
                "WebFetch `{}` returned HTTP {}. Check the URL or fetch a more specific page.",
                final_url,
                status.as_u16()
            )));
        }

        if response
            .content_length()
            .is_some_and(|len| len as usize > HARD_MAX_BYTES)
        {
            return Err(ToolError::Execution(format!(
                "WebFetch `{}` is too large (content-length exceeds {} bytes). Use a more \
                 specific URL.",
                final_url, HARD_MAX_BYTES
            )));
        }

        let bytes = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = response.bytes() => result.map_err(|err| {
                ToolError::Execution(format!("WebFetch could not read `{final_url}`: {err}."))
            })?,
        };
        let kept_len = bytes.len().min(max_bytes);
        let text = String::from_utf8_lossy(&bytes[..kept_len]);
        let mut rendered = String::new();
        rendered.push_str("url: ");
        rendered.push_str(final_url.as_str());
        rendered.push_str("\nstatus: ");
        rendered.push_str(&status.as_u16().to_string());
        if let Some(content_type) = &content_type {
            rendered.push_str("\ncontent_type: ");
            rendered.push_str(content_type);
        }
        rendered.push_str("\n\n");
        rendered.push_str(text.as_ref());
        if kept_len < bytes.len() {
            rendered.push_str("\n\n[... response truncated by max_bytes ...]");
        }

        let (rendered, output_truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);
        Ok(ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "url": final_url.as_str(),
                "status": status.as_u16(),
                "content_type": content_type,
                "bytes": bytes.len(),
                "kept_bytes": kept_len,
                "truncated": output_truncated || kept_len < bytes.len(),
            })),
        })
    }
}
