//! `search_web` tool: query a web search engine and return formatted results.
//!
//! Powered by a swappable [`SearchBackend`]; the default implementation
//! uses a fallback chain (DuckDuckGo → SearXNG). Results are returned as a
//! token-efficient markdown list with titles, URLs, and snippets. An optional
//! [`SearchReranker`] re-orders results by relevance.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::rerank::SearchReranker;
use crate::search_backend::{SearchBackend, SearchError};

/// Maximum characters in the formatted output passed back to the model.
const MAX_OUTPUT_CHARS: usize = 60_000;

/// Default maximum number of results to include in the output.
const DEFAULT_MAX_RESULTS: usize = 15;

/// Absolute cap on the number of results.
const HARD_MAX_RESULTS: usize = 20;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchWebInput {
    /// The search query string. Be specific: include keywords, dates, or
    /// site restrictions (e.g. `site:docs.rs`) for better results.
    query: String,
    /// How many results to return (1–20, default 15). Fewer results are
    /// faster but less comprehensive; more results give broader coverage.
    max_results: Option<usize>,
    /// Search depth: `"broad"` for overview queries that cast a wide net,
    /// `"specific"` for targeted searches on a narrow topic. Affects query
    /// interpretation; the model should use `"broad"` when exploring a
    /// new domain and `"specific"` when drilling into known topics.
    #[allow(dead_code)]
    depth: Option<String>,
}

/// Searches the web via a pluggable backend and returns results as markdown.
pub struct SearchWebTool {
    backend: Arc<dyn SearchBackend>,
    reranker: Option<Arc<dyn SearchReranker>>,
}

impl SearchWebTool {
    /// Create a tool that uses the given backend.
    pub fn new(backend: Arc<dyn SearchBackend>) -> Self {
        Self {
            backend,
            reranker: None,
        }
    }

    /// Attach a result re-ranker that re-orders results by relevance to the query.
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

        let query = parsed.query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput(
                "`query` must be a non-empty search string for `search_web`.".to_owned(),
            ));
        }

        let max_results = parsed
            .max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, HARD_MAX_RESULTS);

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
                Err(SearchError::ParseError(msg)) => {
                    return Err(ToolError::Execution(format!(
                        "Search backend returned unparseable response: {msg}. Try a different query."
                    )));
                }
            },
        };

        // Apply re-ranker if configured.
        let reranked = if let Some(ref reranker) = self.reranker {
            reranker.rerank(query, &results)
        } else {
            results
        };

        let truncated: Vec<_> = reranked.into_iter().take(max_results).collect();
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
