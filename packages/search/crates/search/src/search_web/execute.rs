use std::sync::Arc;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{ToolContext, ToolError};

use crate::rerank::SearchReranker;
use crate::search_backend::{SearchBackend, SearchError};

use super::format::{format_search_results, truncate_chars};

pub(crate) async fn execute(
    backend: &Arc<dyn SearchBackend>,
    reranker: &Option<Arc<dyn SearchReranker>>,
    ctx: ToolContext,
    query: String,
    max_results: usize,
    max_output_chars: usize,
) -> Result<ToolOutput, ToolError> {
    let results = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
        result = backend.search(&query) => match result {
            Ok(r) => r,
            Err(SearchError::RateLimited) => {
                return Err(ToolError::Execution(
                    "All configured search backends are rate-limited right now. Wait a \
                     moment and retry, or set `BRAVE_SEARCH_API_KEY` (Brave Search) / \
                     `SEARXNG_BASE_URL` (your own SearXNG) for a more reliable backend."
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

    let reranked = if let Some(reranker) = reranker {
        reranker.rerank(&query, &results)
    } else {
        results
    };

    let truncated: Vec<_> = reranked.into_iter().take(max_results).collect();
    let result_count = truncated.len();
    let rendered = format_search_results(&query, &truncated);
    let (rendered, output_truncated) = truncate_chars(&rendered, max_output_chars);

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
