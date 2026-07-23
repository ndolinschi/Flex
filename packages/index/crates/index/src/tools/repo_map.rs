use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{AgentEvent, ToolOutput, ToolResultBlock};
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::repomap::build_repo_map_cached;
use crate::tools::shared::{IndexOpenMode, open_and_build_with_events_mode};

const DEFAULT_BUDGET: usize = 2_000;
const MIN_BUDGET: usize = 200;
const MAX_BUDGET: usize = 8_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RepoMapInput {
    token_budget: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct RepoMapTool {
    open_mode: IndexOpenMode,
}

impl Default for RepoMapTool {
    fn default() -> Self {
        Self::new(IndexOpenMode::default())
    }
}

impl RepoMapTool {
    pub fn new(open_mode: IndexOpenMode) -> Self {
        Self { open_mode }
    }
}

#[async_trait]
impl Tool for RepoMapTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "RepoMap".to_owned(),
            description: "Return a compact map of the repository: the most central files \
                 (PageRank over the import graph, boosted by symbol density) with their \
                 key symbols. Call at most once per project/workspace when you need \
                 orientation in an unfamiliar repo — the result is cached on disk and \
                 reused across chats until the index changes. Prefer `SearchCode` / \
                 `FindSymbol` / `Read` when you already know what to look for. Set \
                 `token_budget` to control how much of the map fits (default 2000 \
                 tokens, max 8000)."
                .to_owned(),
            input_schema: schema_of::<RepoMapInput>(),
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
        let parsed: RepoMapInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `RepoMap` must be {{\"token_budget\": <optional number>}}: {err}."
            ))
        })?;

        let budget = parsed
            .token_budget
            .unwrap_or(DEFAULT_BUDGET)
            .clamp(MIN_BUDGET, MAX_BUDGET);
        let cwd = ctx.cwd.clone();
        let events = ctx.events.clone();
        let call_id = ctx.call_id.clone();
        let cancel = ctx.cancel.clone();
        let open_mode = self.open_mode;
        let handle = tokio::task::spawn_blocking(move || {
            run_map(&cwd, budget, &events, &call_id, open_mode)
        });
        let (rendered, file_count, cache_hit) = tokio::select! {
            _ = cancel.cancelled() => return Err(ToolError::Cancelled),
            result = handle => result.map_err(|err| {
                ToolError::Execution(format!("RepoMap worker failed before producing results: {err}."))
            })??,
        };

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(rendered.clone())],
            is_error: false,
            structured: Some(serde_json::json!({
                "token_budget": budget,
                "chars": rendered.len(),
                "file_count": file_count,
                "cache_hit": cache_hit,
            })),
        })
    }
}

fn run_map(
    cwd: &std::path::Path,
    budget: usize,
    events: &agentloop_core::EventSink,
    call_id: &agentloop_contracts::ToolCallId,
    open_mode: IndexOpenMode,
) -> Result<(String, usize, bool), ToolError> {
    let store = open_and_build_with_events_mode(cwd, events, Some(call_id), open_mode)?;
    let file_count = store.indexed_file_count();
    if file_count == 0 {
        events.emit(AgentEvent::ToolProgress {
            call_id: call_id.clone(),
            note: "Repo map: no indexed files yet".to_owned(),
        });
    } else {
        events.emit(AgentEvent::ToolProgress {
            call_id: call_id.clone(),
            note: format!("Building repo map… {file_count} files"),
        });
    }
    let (map, count, cache_hit) = build_repo_map_cached(&store, budget);
    if cache_hit {
        events.emit(AgentEvent::ToolProgress {
            call_id: call_id.clone(),
            note: format!("Repo map ready · {count} files (cached)"),
        });
    } else if count > 0 {
        events.emit(AgentEvent::ToolProgress {
            call_id: call_id.clone(),
            note: format!("Repo map ready · {count} files"),
        });
    }
    Ok((map, count, cache_hit))
}

fn schema_of<I: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use agentloop_contracts::{SessionId, ToolCallId, TurnId};
    use agentloop_core::{EventSink, Tool};
    use tokio_util::sync::CancellationToken;

    use crate::tools::shared::{
        lock_index_root_override, open_and_build_in, set_index_root_override,
    };

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, content).unwrap_or_else(|e| panic!("{e}"));
    }

    fn tool_ctx(cwd: &Path) -> ToolContext {
        let (events, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId::from("sess-map"),
            turn_id: TurnId::from("turn-map"),
            call_id: ToolCallId::from("call-map"),
            cwd: cwd.to_path_buf(),
            cancel: CancellationToken::new(),
            events,
        }
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn repo_map_tool_returns_hub_file() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(repo.path(), "src/core.rs", "pub fn important() {}\n");
        write(
            repo.path(),
            "src/a.rs",
            "use crate::core::important;\npub fn a() {}\n",
        );
        write(
            repo.path(),
            "src/b.rs",
            "use crate::core::important;\npub fn b() {}\n",
        );

        let _ = open_and_build_in(repo.path(), index_root.path()).unwrap_or_else(|e| panic!("{e}"));

        let _gate = lock_index_root_override();
        set_index_root_override(Some(index_root.path().to_path_buf()));
        let output = RepoMapTool::default()
            .run(tool_ctx(repo.path()), serde_json::json!({}))
            .await;
        set_index_root_override(None);
        drop(_gate);

        let output = output.unwrap_or_else(|e| panic!("{e}"));
        assert!(!output.is_error);
        let text = match &output.content[0] {
            ToolResultBlock::Markdown { text } => text,
            other => panic!("expected markdown, got {other:?}"),
        };
        assert!(text.contains("src/core.rs"), "map missing hub: {text}");
        let structured = output.structured.expect("structured");
        assert!(
            structured
                .get("file_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                >= 3,
            "{structured}"
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn repo_map_tool_cache_hit_on_second_call() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(repo.path(), "src/lib.rs", "pub fn lib() {}\n");

        let _ = open_and_build_in(repo.path(), index_root.path()).unwrap_or_else(|e| panic!("{e}"));

        let _gate = lock_index_root_override();
        set_index_root_override(Some(index_root.path().to_path_buf()));
        let first = RepoMapTool::default()
            .run(tool_ctx(repo.path()), serde_json::json!({}))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        let second = RepoMapTool::default()
            .run(tool_ctx(repo.path()), serde_json::json!({}))
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        set_index_root_override(None);
        drop(_gate);

        assert_eq!(
            first.structured.as_ref().and_then(|s| s.get("cache_hit")),
            Some(&serde_json::json!(false))
        );
        assert_eq!(
            second.structured.as_ref().and_then(|s| s.get("cache_hit")),
            Some(&serde_json::json!(true))
        );
    }
}
