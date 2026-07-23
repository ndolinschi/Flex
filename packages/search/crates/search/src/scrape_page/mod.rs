mod body;
mod extract;
mod fetch;
mod strip;

use async_trait::async_trait;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use extract::schema_of;
use fetch::ScrapePageInput;

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
