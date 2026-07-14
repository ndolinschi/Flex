//! `scrape_page` tool: fetch a URL and return its text content.
//!
//! Follows the same pattern as the engine's `WebFetch` tool: reqwest for
//! HTTP, `htmd` for HTML-to-markdown conversion, and truncation with
//! explicit markers so the model knows content was cut.
//!
//! After markdown conversion, a heuristic extracts the "content core" — the
//! largest contiguous block of paragraphs — to keep the output token-efficient
//! by dropping boilerplate (nav, footer, sidebars) from the response.

mod body;
mod extract;
mod fetch;
mod strip;

use async_trait::async_trait;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use extract::schema_of;
use fetch::ScrapePageInput;

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
                 different URL. When `include_links` is true (the default), a numbered list \
                 of links found on the page is appended."
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
        fetch::scrape_page(&self.client, ctx, input).await
    }
}
