mod execute;
mod format;

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::rerank::SearchReranker;
use crate::search_backend::SearchBackend;

use format::schema_of;

const MAX_OUTPUT_CHARS: usize = 60_000;

const DEFAULT_MAX_RESULTS: usize = 15;

const HARD_MAX_RESULTS: usize = 20;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchWebInput {
    query: String,

    max_results: Option<usize>,

    depth: Option<String>,
}

pub struct SearchWebTool {
    backend: Arc<dyn SearchBackend>,
    reranker: Option<Arc<dyn SearchReranker>>,
}

impl SearchWebTool {
    pub fn new(backend: Arc<dyn SearchBackend>) -> Self {
        Self {
            backend,
            reranker: None,
        }
    }

    pub fn with_reranker(mut self, reranker: Arc<dyn SearchReranker>) -> Self {
        self.reranker = Some(reranker);
        self
    }
}

#[async_trait]
impl Tool for SearchWebTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "search_web".to_owned(),
            description: "Search the web and return a list of results (title, URL, snippet). \
                 Use this to find current information, documentation, news, or any \
                 topic beyond your training data. Be specific with your query: include \
                 relevant keywords, dates, or operators like `site:` for better \
                 results. Set `max_results` (1–20, default 15) to control how many \
                 results you get back. Set `depth` to `\"broad\"` for exploratory \
                 overview searches or `\"specific\"` for narrowly targeted queries. \
                 If you need to explore a result further, call `scrape_page` on its URL."
                .to_owned(),
            input_schema: schema_of::<SearchWebInput>(),
            read_only: true,
            category: ToolCategory::Web,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let parsed: SearchWebInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `search_web` must be {{\"query\": \"<search string>\"}}: {err}."
            ))
        })?;

        let raw_query = parsed.query.trim();
        if raw_query.is_empty() {
            return Err(ToolError::InvalidInput(
                "`query` must be a non-empty search string for `search_web`.".to_owned(),
            ));
        }

        let query = if parsed.depth.as_deref() == Some("broad") {
            format!("{raw_query} overview")
        } else {
            raw_query.to_owned()
        };

        let max_results = parsed
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, HARD_MAX_RESULTS);

        execute::execute(
            &self.backend,
            &self.reranker,
            ctx,
            query,
            max_results,
            MAX_OUTPUT_CHARS,
        )
        .await
    }
}
