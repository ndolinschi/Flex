//! `scrape_page` tool: fetch a URL and return its text content.
//!
//! Follows the same pattern as the engine's `WebFetch` tool: reqwest for
//! HTTP, `htmd` for HTML-to-markdown conversion, and truncation with
//! explicit markers so the model knows content was cut.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

/// Default max response bytes to keep.
const DEFAULT_MAX_BYTES: usize = 200_000;

/// Hard cap on response bytes fetched.
const HARD_MAX_BYTES: usize = 1_000_000;

/// Maximum characters in the output fed back to the model.
const MAX_OUTPUT_CHARS: usize = 120_000;

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ScrapePageInput {
    /// HTTP or HTTPS URL of the page to scrape.
    url: String,
    /// Maximum response bytes to keep. Defaults to 200000, capped at 1000000.
    max_bytes: Option<usize>,
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// Fetches a web page and returns its text content as markdown.
///
/// HTML pages are converted to markdown via `htmd`; non-HTML content is
/// returned as plain text. The output is truncated with an explicit marker
/// so the model can decide whether to refine the query.
#[derive(Clone)]
pub struct ScrapePageTool {
    client: reqwest::Client,
}

impl ScrapePageTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for ScrapePageTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ScrapePageTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "scrape_page".to_owned(),
            description: "Fetch and read the text content of a web page. Returns the page content \
                 converted to readable markdown (HTML pages) or plain text (other content \
                 types). Use this after `search_web` to read the full content of a result. \
                 Set `max_bytes` to limit large pages. Non-success HTTP statuses are \
                 returned as tool errors so you can decide whether to retry with a \
                 different URL."
                .to_owned(),
            input_schema: schema_of::<ScrapePageInput>(),
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
        let parsed: ScrapePageInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `scrape_page` must be {{\"url\": \"https://...\", \"max_bytes\": \
                 <optional number>}}: {err}."
            ))
        })?;

        let url = reqwest::Url::parse(&parsed.url).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`url` is not a valid absolute URL for `scrape_page`: {err}. Pass an \
                 http:// or https:// URL."
            ))
        })?;

        if !matches!(url.scheme(), "http" | "https") {
            return Err(ToolError::InvalidInput(format!(
                "`scrape_page` supports only http:// and https:// URLs, but got `{}`.",
                url.scheme()
            )));
        }

        let max_bytes = parsed
            .max_bytes
            .unwrap_or(DEFAULT_MAX_BYTES)
            .min(HARD_MAX_BYTES);

        let response = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = self.client.get(url.clone()).send() => result.map_err(|err| {
                ToolError::Execution(format!(
                    "`scrape_page` request to `{url}` failed: {err}."
                ))
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
                "`scrape_page` returned HTTP {} for `{}`. Check the URL or fetch a more \
                 specific page.",
                status.as_u16(),
                final_url
            )));
        }

        if response
            .content_length()
            .is_some_and(|len| len as usize > HARD_MAX_BYTES)
        {
            return Err(ToolError::Execution(format!(
                "`scrape_page` URL `{}` is too large (content-length exceeds {} bytes). \
                 Use a more specific URL.",
                final_url, HARD_MAX_BYTES
            )));
        }

        let bytes = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = response.bytes() => result.map_err(|err| {
                ToolError::Execution(format!(
                    "`scrape_page` could not read `{final_url}`: {err}."
                ))
            })?,
        };

        let kept_len = bytes.len().min(max_bytes);
        let is_html = content_type
            .as_deref()
            .is_some_and(|ct| ct.contains("text/html") || ct.contains("application/xhtml"));

        let text = if is_html {
            // Convert HTML to markdown via htmd.
            let html_str = String::from_utf8_lossy(&bytes[..kept_len]);
            htmd::convert(&html_str).unwrap_or_else(|_| html_str.into_owned())
        } else {
            String::from_utf8_lossy(&bytes[..kept_len]).into_owned()
        };

        let mut rendered = String::new();
        rendered.push_str("url: ");
        rendered.push_str(final_url.as_str());
        rendered.push_str("\nstatus: ");
        rendered.push_str(&status.as_u16().to_string());
        if let Some(ref ct) = content_type {
            rendered.push_str("\ncontent_type: ");
            rendered.push_str(ct);
        }
        rendered.push_str("\n\n");
        rendered.push_str(&text);

        if kept_len < bytes.len() {
            rendered.push_str("\n\n[... response truncated by max_bytes ...]");
        }

        let (rendered, output_truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(rendered)],
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

// ---------------------------------------------------------------------------
// Local helpers
// ---------------------------------------------------------------------------

fn schema_of<I: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n\n[... output truncated ...]");
    (out, true)
}
