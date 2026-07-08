//! `search_web` tool: query a web search engine and return formatted results.
//!
//! Powered by a swappable [`SearchBackend`]; the default implementation
//! queries DuckDuckGo's HTML endpoint (no API key required). Results are
//! returned as a token-efficient markdown list with titles, URLs, and snippets.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::search_backend::{SearchBackend, SearchError};

/// Maximum characters in the formatted output passed back to the model.
const MAX_OUTPUT_CHARS: usize = 60_000;

/// Maximum number of results to include in the output.
const MAX_RESULTS: usize = 15;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchWebInput {
    /// The search query string. Be specific: include keywords, dates, or
    /// site restrictions (e.g. `site:docs.rs`) for better results.
    query: String,
}

/// Searches the web via a pluggable backend and returns results as markdown.
pub struct SearchWebTool {
    backend: Arc<dyn SearchBackend>,
}

impl SearchWebTool {
    /// Create a tool that uses the given backend.
    pub fn new(backend: Arc<dyn SearchBackend>) -> Self {
        Self { backend }
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
                 results. Results are limited to the most relevant matches; if you \
                 need to explore a result further, call `scrape_page` on its URL."
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

        let query = parsed.query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput(
                "`query` must be a non-empty search string for `search_web`.".to_owned(),
            ));
        }

        let results = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = self.backend.search(query) => match result {
                Ok(r) => r,
                Err(SearchError::RateLimited) => {
                    return Err(ToolError::Execution(
                        "The search backend is rate-limited. Wait a moment and try again with \
                         a slightly different or more specific query."
                            .to_owned(),
                    ));
                }
                Err(SearchError::NoResults) => {
                    return Ok(ToolOutput {
                        content: vec![ToolResultBlock::markdown(format!(
                            "## Search results for \"{query}\"\n\nNo results found. Try \
                             refining your query with more specific keywords or different \
                             phrasing."
                        ))],
                        is_error: false,
                        structured: Some(serde_json::json!({
                            "query": query,
                            "results": [],
                        })),
                    });
                }
                Err(SearchError::Request(err)) => {
                    return Err(ToolError::Execution(format!(
                        "Search request failed: {err}. Check your network connection and retry."
                    )));
                }
            },
        };

        let truncated: Vec<_> = results.into_iter().take(MAX_RESULTS).collect();
        let result_count = truncated.len();
        let rendered = format_search_results(query, &truncated);

        let (rendered, output_truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "query": query,
                "result_count": result_count,
                "truncated": output_truncated,
                "results": truncated.iter().map(|r| serde_json::json!({
                    "title": r.title,
                    "url": r.url,
                    "snippet": r.snippet,
                })).collect::<Vec<_>>(),
            })),
        })
    }
}

fn format_search_results(query: &str, results: &[crate::search_backend::SearchResult]) -> String {
    let mut out = String::new();
    out.push_str("## Search results for \"");
    out.push_str(query);
    out.push_str("\"\n\n");
    for (i, result) in results.iter().enumerate() {
        out.push_str(&(i + 1).to_string());
        out.push_str(". [");
        out.push_str(&result.title);
        out.push_str("](");
        out.push_str(&result.url);
        out.push_str(")\n   ");
        out.push_str(&result.snippet);
        out.push('\n');
    }
    out
}

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
