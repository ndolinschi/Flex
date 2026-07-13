//! Auto-context hook: on `UserPromptSubmit`, retrieve top-k code chunks
//! matching the prompt and append them to the first-iteration user message.
//!
//! Soft-fail by design — index/open/search errors are logged and the turn
//! continues without injected context. Gated by [`crate::IndexPlugin`]'s
//! `auto_context` flag (env [`AUTO_CONTEXT_ENV`] or desktop prefs);
//! default is **off**.

use std::path::PathBuf;

use async_trait::async_trait;

use agentloop_contracts::{ContentBlock, HookPoint};
use agentloop_core::{Hook, HookContext, HookData, HookError, HookOutcome};

use crate::retrieve::{Hit, search_hybrid};
use crate::store::{IndexStore, StoreError as IndexStoreError, UpdateStats};
use crate::tools::shared::{index_dir_for, index_root_base, open_and_build};

/// Env var that enables auto-context when set to a truthy value
/// (`1`/`true`/`on`/`yes`, case-insensitive). Used by
/// [`crate::IndexPlugin::default`].
pub const AUTO_CONTEXT_ENV: &str = "AGENTLOOP_AUTO_CONTEXT";

const DEFAULT_K: usize = 5;
const MAX_SNIPPET_CHARS: usize = 240;

/// Injects top-k hybrid-search hits for the submitted prompt into the
/// turn's first user message.
#[derive(Debug, Clone)]
pub struct AutoContextHook {
    enabled: bool,
    k: usize,
}

impl AutoContextHook {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            k: DEFAULT_K,
        }
    }

    pub fn with_k(mut self, k: usize) -> Self {
        self.k = k.clamp(1, 20);
        self
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }
}

#[async_trait]
impl Hook for AutoContextHook {
    fn interests(&self) -> &[HookPoint] {
        &[HookPoint::UserPromptSubmit]
    }

    async fn on(
        &self,
        _point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError> {
        if !self.enabled {
            return Ok(HookOutcome::Continue);
        }
        let HookData::UserPrompt { input } = &mut ctx.data else {
            return Ok(HookOutcome::Continue);
        };

        let prompt_text = input.joined_text();
        let trimmed = prompt_text.trim();
        if trimmed.is_empty() {
            return Ok(HookOutcome::Continue);
        }

        let Some(store) = ctx.store.clone() else {
            tracing::debug!("auto-context: no session store; skipping");
            return Ok(HookOutcome::Continue);
        };

        let cwd = match store.get_meta(ctx.session_id).await {
            Ok(meta) => meta.cwd,
            Err(err) => {
                tracing::debug!(error = %err, "auto-context: could not load session meta");
                return Ok(HookOutcome::Continue);
            }
        };

        let k = self.k;
        let query = trimmed.to_owned();
        let cwd_owned = cwd;
        let hits =
            match tokio::task::spawn_blocking(move || fetch_hits(&cwd_owned, &query, k)).await {
                Ok(Ok(hits)) => hits,
                Ok(Err(err)) => {
                    tracing::debug!(error = %err, "auto-context: retrieval failed");
                    return Ok(HookOutcome::Continue);
                }
                Err(err) => {
                    tracing::debug!(error = %err, "auto-context: worker join failed");
                    return Ok(HookOutcome::Continue);
                }
            };

        if hits.is_empty() {
            return Ok(HookOutcome::Continue);
        }

        input
            .parts
            .push(ContentBlock::markdown(render_context_block(&hits)));
        Ok(HookOutcome::Mutated)
    }
}

fn fetch_hits(cwd: &std::path::Path, query: &str, k: usize) -> Result<Vec<Hit>, String> {
    let store = open_and_build(cwd).map_err(|e| e.to_string())?;
    search_hybrid(&store, query, k).map_err(|e| e.to_string())
}

fn render_context_block(hits: &[Hit]) -> String {
    let mut out = String::from(
        "[auto-context]\nRelevant indexed snippets for this turn (not user-authored):\n",
    );
    for hit in hits {
        let symbol = hit.symbol.as_deref().unwrap_or("-");
        let snippet = truncate_chars(&hit.snippet, MAX_SNIPPET_CHARS);
        out.push_str(&format!(
            "- {}:{}-{} — {} — {}\n",
            hit.path, hit.start_line, hit.end_line, symbol, snippet
        ));
    }
    out
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// Parse a truthy env value for [`AUTO_CONTEXT_ENV`].
pub fn env_auto_context_enabled() -> bool {
    match std::env::var(AUTO_CONTEXT_ENV) {
        Ok(raw) => {
            let v = raw.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "on" | "yes")
        }
        Err(_) => false,
    }
}

/// Public status snapshot for desktop polling (no AgentEvent).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub repo_root: PathBuf,
    pub index_dir: PathBuf,
    pub file_count: usize,
    pub symbol_count: usize,
    pub embedded_chunk_count: usize,
    /// True when the on-disk index exists and has at least one file.
    pub ready: bool,
}

/// Open (without rebuilding) and report status for `cwd`'s index.
pub fn status_for(cwd: &std::path::Path) -> Result<IndexStatus, IndexStoreError> {
    let index_dir = index_dir_for(cwd, &index_root_base());
    let repo_root = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    if !index_dir.exists() {
        return Ok(IndexStatus {
            repo_root,
            index_dir,
            file_count: 0,
            symbol_count: 0,
            embedded_chunk_count: 0,
            ready: false,
        });
    }
    let store = IndexStore::open(&repo_root, &index_dir)?;
    let file_count = store.indexed_file_count();
    Ok(IndexStatus {
        repo_root,
        index_dir: store.index_dir().to_path_buf(),
        file_count,
        symbol_count: store.symbols().len(),
        embedded_chunk_count: store.embedded_chunk_count(),
        ready: file_count > 0,
    })
}

/// Build or incrementally update the index for `cwd`, returning status + stats.
pub fn rebuild_with_stats(cwd: &std::path::Path) -> Result<(IndexStatus, UpdateStats), String> {
    use crate::embed::resolve_embedder;

    let index_dir = index_dir_for(cwd, &index_root_base());
    let embedder = resolve_embedder(&index_dir).map_err(|e| e.to_string())?;
    let mut store = match embedder {
        Some(provider) => IndexStore::open_with_embeddings(cwd, &index_dir, provider)
            .map_err(|e| e.to_string())?,
        None => IndexStore::open(cwd, &index_dir).map_err(|e| e.to_string())?,
    };
    let stats = store.build().map_err(|e| e.to_string())?;
    let status = IndexStatus {
        repo_root: store.repo_root().to_path_buf(),
        index_dir: store.index_dir().to_path_buf(),
        file_count: store.indexed_file_count(),
        symbol_count: store.symbols().len(),
        embedded_chunk_count: store.embedded_chunk_count(),
        ready: store.indexed_file_count() > 0,
    };
    Ok((status, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    use agentloop_contracts::{
        AgentEvent, CheckpointRef, CompactionSummary, PromptInput, SessionId, SessionMeta,
        SessionMetaPatch, TurnId,
    };
    use agentloop_core::{
        Hook, HookContext, HookData, HookOutcome, SessionStore, StoreError, StoredEvent,
    };

    use crate::tools::shared::{lock_index_root_override, set_index_root_override};

    struct MemStore {
        meta: SessionMeta,
    }

    #[async_trait]
    impl SessionStore for MemStore {
        async fn create(&self, _meta: SessionMeta) -> Result<(), StoreError> {
            Ok(())
        }
        async fn append(&self, _id: &SessionId, _events: &[AgentEvent]) -> Result<u64, StoreError> {
            Ok(0)
        }
        async fn read(
            &self,
            _id: &SessionId,
            _from_seq: u64,
        ) -> Result<Vec<StoredEvent>, StoreError> {
            Ok(Vec::new())
        }
        async fn list(&self) -> Result<Vec<SessionMeta>, StoreError> {
            Ok(vec![self.meta.clone()])
        }
        async fn get_meta(&self, _id: &SessionId) -> Result<SessionMeta, StoreError> {
            Ok(self.meta.clone())
        }
        async fn update_meta(
            &self,
            _id: &SessionId,
            _patch: SessionMetaPatch,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn delete(&self, _id: &SessionId) -> Result<(), StoreError> {
            Ok(())
        }
        async fn record_compaction(
            &self,
            _id: &SessionId,
            _compaction: CompactionSummary,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn record_checkpoint(
            &self,
            _id: &SessionId,
            _checkpoint: CheckpointRef,
        ) -> Result<(), StoreError> {
            Ok(())
        }
        async fn list_checkpoints(
            &self,
            _id: &SessionId,
        ) -> Result<Vec<CheckpointRef>, StoreError> {
            Ok(Vec::new())
        }
    }

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{e}"));
        }
        fs::write(path, content).unwrap_or_else(|e| panic!("{e}"));
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn auto_context_appends_hits_when_enabled() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        write(
            repo.path(),
            "src/session_title.rs",
            "pub fn generate_session_title(msg: &str) -> String { msg.to_owned() }\n",
        );

        let _gate = lock_index_root_override();
        set_index_root_override(Some(index_root.path().to_path_buf()));

        let _ = open_and_build(repo.path()).unwrap_or_else(|e| panic!("{e}"));
        assert!(index_dir_for(repo.path(), index_root.path()).exists());

        let session = SessionId::from("sess-ac");
        let store: Arc<dyn SessionStore> = Arc::new(MemStore {
            meta: SessionMeta {
                id: session.clone(),
                title: None,
                agent_id: "native".to_owned(),
                parent_id: None,
                role: None,
                depth: 0,
                provider_session_id: None,
                cwd: repo.path().to_path_buf(),
                model: None,
                fallback_models: Vec::new(),
                mode: None,
                isolation: None,
                workspace_id: None,
                executor: None,
                base_cwd: None,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        });

        let hook = AutoContextHook::new(true);
        let mut input = PromptInput::text("where is the session title generated");
        let mut ctx = HookContext {
            session_id: &session,
            turn_id: Some(&TurnId::from("turn-1")),
            data: HookData::UserPrompt { input: &mut input },
            store: Some(store),
            events: None,
        };
        let outcome = hook
            .on(HookPoint::UserPromptSubmit, &mut ctx)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        set_index_root_override(None);
        drop(_gate);

        assert_eq!(outcome, HookOutcome::Mutated);
        assert!(
            input.parts.iter().any(|p| matches!(
                p,
                ContentBlock::Markdown { text } if text.contains("[auto-context]")
                    && text.contains("session_title")
            )),
            "expected auto-context block, got {:?}",
            input.parts
        );
    }

    #[tokio::test]
    async fn auto_context_disabled_is_noop() {
        let session = SessionId::from("sess-off");
        let hook = AutoContextHook::new(false);
        let mut input = PromptInput::text("anything");
        let mut ctx = HookContext {
            session_id: &session,
            turn_id: None,
            data: HookData::UserPrompt { input: &mut input },
            store: None,
            events: None,
        };
        let outcome = hook
            .on(HookPoint::UserPromptSubmit, &mut ctx)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(outcome, HookOutcome::Continue);
        assert_eq!(input.parts.len(), 1);
    }
}
