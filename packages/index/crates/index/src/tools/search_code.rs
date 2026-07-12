//! `SearchCode` tool: hybrid (BM25 + symbol-boost, fused with cosine vector
//! rank when embeddings are enabled) search over the repo at the session's
//! cwd, returning a compact, token-frugal ranked list.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::retrieve::{Hit, search_hybrid};
use crate::tools::shared::open_and_build;

/// Cap on total rendered output, chosen to stay well under ~2k tokens
/// (roughly 4 chars/token, so ~8000 chars is a comfortable ceiling).
const MAX_OUTPUT_CHARS: usize = 8_000;

const DEFAULT_K: usize = 8;
const MAX_K: usize = 30;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchCodeInput {
    /// Natural-language or keyword query, e.g. "where is the session title
    /// generated" or "retry logic for provider requests".
    query: String,
    /// Number of ranked hits to return (default 8, capped at 30).
    k: Option<usize>,
}

/// Lexical + symbol code search over the session's working directory.
///
/// Builds (or incrementally updates) a per-repo BM25 index on first use,
/// then returns a compact ranked list of `path:start-end — symbol —
/// snippet` lines.
#[derive(Debug, Default, Clone, Copy)]
pub struct SearchCodeTool;

#[async_trait]
impl Tool for SearchCodeTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "SearchCode".to_owned(),
            description: "Search the codebase for code relevant to a natural-language or keyword \
                 query, e.g. \"where is the session title generated\" or \"retry logic for \
                 provider requests\". Returns a compact ranked list of \
                 `path:start-end — symbol — snippet` results, best matches first. Builds a \
                 lexical + symbol index of the repo on first use (a one-time cost); later calls \
                 in the same repo reuse and incrementally update it. Prefer this over `Grep` \
                 when you don't know the exact identifier or string to search for — `Grep` is \
                 better once you know precisely what text to match. Set `k` to control how many \
                 results come back (default 8, max 30)."
                .to_owned(),
            input_schema: schema_of::<SearchCodeInput>(),
            read_only: true,
            category: ToolCategory::Fs,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let parsed: SearchCodeInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `SearchCode` must be {{\"query\": \"<search text>\", \"k\": \
                 <optional number>}}: {err}."
            ))
        })?;

        let query = parsed.query.trim();
        if query.is_empty() {
            return Err(ToolError::InvalidInput(
                "`query` must be a non-empty string for `SearchCode`.".to_owned(),
            ));
        }
        let k = parsed.k.unwrap_or(DEFAULT_K).clamp(1, MAX_K);
        let cwd = ctx.cwd.clone();
        let query_owned = query.to_owned();

        let cancel = ctx.cancel.clone();
        let handle = tokio::task::spawn_blocking(move || run_search(&cwd, &query_owned, k));
        let hits = tokio::select! {
            _ = cancel.cancelled() => return Err(ToolError::Cancelled),
            result = handle => result.map_err(|err| {
                ToolError::Execution(format!("SearchCode worker failed before producing results: {err}."))
            })??,
        };

        let rendered = render_hits(query, &hits);
        let (rendered, truncated) = truncate_chars(&rendered, MAX_OUTPUT_CHARS);

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(serde_json::json!({
                "query": query,
                "hit_count": hits.len(),
                "truncated": truncated,
                "hits": hits.iter().map(|h| serde_json::json!({
                    "path": h.path,
                    "start_line": h.start_line,
                    "end_line": h.end_line,
                    "symbol": h.symbol,
                    "score": h.score,
                })).collect::<Vec<_>>(),
            })),
        })
    }
}

fn run_search(cwd: &std::path::Path, query: &str, k: usize) -> Result<Vec<Hit>, ToolError> {
    let store = open_and_build(cwd)?;
    search_hybrid(&store, query, k)
        .map_err(|err| ToolError::Execution(format!("SearchCode retrieval failed: {err}.")))
}

fn render_hits(query: &str, hits: &[Hit]) -> String {
    if hits.is_empty() {
        return format!(
            "No matches for \"{query}\". Try different keywords, or use `Grep` if you know the \
             exact identifier or string."
        );
    }
    let mut out = String::new();
    for hit in hits {
        let symbol = hit.symbol.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "{}:{}-{} — {} — {}\n",
            hit.path, hit.start_line, hit.end_line, symbol, hit.snippet
        ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::Instant;

    use agentloop_contracts::{SessionId, ToolCallId, TurnId};
    use agentloop_core::{EventSink, Tool};
    use tokio_util::sync::CancellationToken;

    use crate::tools::shared::{
        index_dir_for, index_root_base, lock_index_root_override, open_and_build_in,
        set_index_root_override,
    };

    #[test]
    fn render_hits_empty() {
        let rendered = render_hits("foo", &[]);
        assert!(rendered.contains("No matches"));
    }

    #[test]
    fn render_hits_formats_compactly() {
        let hits = vec![Hit {
            path: "src/session.rs".to_owned(),
            start_line: 10,
            end_line: 20,
            snippet: "fn generate_session_title(...)".to_owned(),
            score: 5.0,
            symbol: Some("generate_session_title".to_owned()),
        }];
        let rendered = render_hits("session title", &hits);
        assert!(rendered.contains("src/session.rs:10-20"));
        assert!(rendered.contains("generate_session_title"));
    }

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, content).unwrap_or_else(|e| panic!("{e}"));
    }

    /// Medium fixture (~200 source files) with one distinctive target file
    /// for live-acceptance ranking + timing.
    fn build_medium_repo(repo: &Path) {
        write(
            repo,
            "src/session_title.rs",
            r#"
/// Derive a short human-readable title for a session from its first user
/// message, truncating to a reasonable length.
pub fn generate_session_title(first_message: &str) -> String {
    first_message.trim().chars().take(60).collect()
}
"#,
        );
        for i in 0..200 {
            write(
                repo,
                &format!("src/filler/mod_{i}.rs"),
                &format!(
                    "/// Filler module {i} with unrelated networking helpers.\npub fn connect_{i}() {{}}\n"
                ),
            );
        }
    }

    fn tool_ctx(cwd: &Path) -> ToolContext {
        let (events, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId::from("sess-live"),
            turn_id: TurnId::from("turn-live"),
            call_id: ToolCallId::from("call-live"),
            cwd: cwd.to_path_buf(),
            cancel: CancellationToken::new(),
            events,
        }
    }

    /// M1/M2 live-acceptance (scripted): build under a scratch app-data root,
    /// assert top-1 ranking + warm-query timing, then exercise the real
    /// `SearchCode` `Tool::run` path with the index-root override so tests
    /// never write to the real platform Application Support directory.
    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // gate must cover Tool::run's spawn_blocking
    async fn live_accept_search_code_ranks_expected_file_top_1() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        build_medium_repo(repo.path());

        let scratch_index = index_dir_for(repo.path(), index_root.path());
        assert!(
            !scratch_index.starts_with(repo.path()),
            "index must not live in the repo: {scratch_index:?}"
        );

        let build_started = Instant::now();
        let store =
            open_and_build_in(repo.path(), index_root.path()).unwrap_or_else(|e| panic!("{e}"));
        let build_ms = build_started.elapsed().as_millis();
        assert!(store.indexed_file_count() >= 201, "expected medium fixture");

        let query_started = Instant::now();
        let hits =
            crate::retrieve::search_hybrid(&store, "where is the session title generated", 5)
                .unwrap_or_else(|e| panic!("{e}"));
        let query_ms = query_started.elapsed().as_millis();
        assert!(!hits.is_empty());
        assert_eq!(
            hits[0].path,
            "src/session_title.rs",
            "expected top-1 session_title.rs, got {:?}",
            hits.iter().map(|h| (&h.path, h.score)).collect::<Vec<_>>()
        );
        assert!(
            query_ms < 500,
            "warm query took {query_ms}ms (M1 target <150ms; soft gate 500ms); build was {build_ms}ms for {} files",
            store.indexed_file_count()
        );

        // Real agent tool-call path (`Tool::run` → spawn_blocking →
        // open_and_build), redirected via the process override so sandbox/CI
        // never needs write access to ~/Library/Application Support.
        let _gate = lock_index_root_override();
        set_index_root_override(Some(index_root.path().to_path_buf()));
        let live_index = index_dir_for(repo.path(), &index_root_base());
        let tool = SearchCodeTool;
        assert_eq!(tool.descriptor().name, "SearchCode");
        assert!(tool.descriptor().read_only);

        let output = tool
            .run(
                tool_ctx(repo.path()),
                serde_json::json!({
                    "query": "where is the session title generated",
                    "k": 5
                }),
            )
            .await;
        set_index_root_override(None);
        drop(_gate);
        let output = output.unwrap_or_else(|e| panic!("SearchCode Tool::run failed: {e}"));
        assert!(!output.is_error);
        let structured = output.structured.expect("structured payload");
        let hit_paths: Vec<&str> = structured["hits"]
            .as_array()
            .expect("hits array")
            .iter()
            .filter_map(|h| h["path"].as_str())
            .collect();
        assert_eq!(
            hit_paths.first().copied(),
            Some("src/session_title.rs"),
            "Tool::run top hit: {hit_paths:?}"
        );
        assert!(
            live_index.exists(),
            "expected index under override app-data at {live_index:?}"
        );
        assert!(
            !live_index.starts_with(repo.path()),
            "live index must not live in the repo"
        );
    }
}
