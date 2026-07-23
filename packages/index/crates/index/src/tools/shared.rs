//! Shared lazy-index-open/build logic for the `SearchCode` and `FindSymbol`
//! tools: both tools need "the index for this session's cwd".
//!
//! `IndexStore::build` is itself incremental (it diffs against the persisted
//! manifest and only re-processes changed files). Every tool call opens the
//! store (cheap — a manifest/symbol JSON read plus an mmap handle). Whether
//! it then calls `build()` depends on [`IndexOpenMode`]:
//! - [`IndexOpenMode::AutoUpdate`] — always scan/update (previous default).
//! - [`IndexOpenMode::ReuseWarm`] — if the on-disk index already has files,
//!   reuse it as-is (no scan, no `IndexingStarted` UI). First use / empty
//!   index still builds. Desktop Settings → Rebuild forces a refresh.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use agentloop_contracts::{AgentEvent, ToolCallId};
use agentloop_core::{EventSink, ToolError};

use crate::embed::resolve_embedder;
use crate::store::{IndexStore, UpdateStats};

/// Repos above this file count get an info-level log line after indexing,
/// since a from-scratch build can take a while and a silent tool call looks
/// hung.
const LARGE_REPO_FILE_THRESHOLD: usize = 20_000;

/// Overrides the app-data *base* used by [`index_root_base`] (the directory
/// under which `agentloop/index/<repo-hash>` is created), so tests and the
/// desktop composition root never touch a real user data dir. Mirrors the
/// env-override pattern used by `providers/{copilot,openai}`; not meant to
/// be set in normal CLI operation.
pub(crate) const INDEX_DIR_OVERRIDE_ENV: &str = "AGENTLOOP_INDEX_DIR";

/// Process-wide override for [`index_root_base`], consulted *before* the
/// env var. Lets tests redirect the tool path to a tempdir without
/// `unsafe` env mutation (`unsafe_code` is forbidden in this workspace).
static INDEX_ROOT_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Env var that enables index auto-update on tool use when set to a truthy
/// value (`1`/`true`/`on`/`yes`, case-insensitive). Used by
/// [`crate::IndexPlugin::default`]. Default is **off** (reuse warm index).
pub const AUTO_UPDATE_ENV: &str = "AGENTLOOP_INDEX_AUTO_UPDATE";

/// Parse a truthy env value for [`AUTO_UPDATE_ENV`].
pub fn env_auto_update_enabled() -> bool {
    match std::env::var(AUTO_UPDATE_ENV) {
        Ok(raw) => {
            let v = raw.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "on" | "yes")
        }
        Err(_) => false,
    }
}

/// Serializes tests that install [`set_index_root_override`]: `Tool::run`
/// dispatches work onto a `spawn_blocking` thread pool, so a thread-local
/// override would be invisible there, and parallel tests would race on the
/// process-wide cell.
#[cfg(test)]
static INDEX_ROOT_OVERRIDE_GATE: Mutex<()> = Mutex::new(());

/// Install (or clear) the process-wide index-root override used by
/// [`index_root_base`]. Test-only: production never calls this.
#[cfg(test)]
pub(crate) fn set_index_root_override(path: Option<PathBuf>) {
    let mut guard = INDEX_ROOT_OVERRIDE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = path;
}

/// Acquire the gate that serializes live-accept tests using the index-root
/// override. Caller must hold the returned guard for the duration of the
/// override (including across `await` points that drive `Tool::run`).
#[cfg(test)]
pub(crate) fn lock_index_root_override() -> std::sync::MutexGuard<'static, ()> {
    INDEX_ROOT_OVERRIDE_GATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Resolve the app-data base directory that owns `agentloop/index/…`:
/// process override (tests) → `$AGENTLOOP_INDEX_DIR` → `$XDG_DATA_HOME` →
/// the platform data dir (`~/Library/Application Support` on macOS,
/// `%LOCALAPPDATA%` on Windows, `~/.local/share` elsewhere) → temp dir.
///
/// Kept separate from [`index_dir_for`] so tests can exercise the latter as
/// a pure function of an explicit `base`.
pub fn index_root_base() -> PathBuf {
    if let Ok(guard) = INDEX_ROOT_OVERRIDE.lock() {
        if let Some(path) = guard.as_ref() {
            return path.clone();
        }
    }
    if let Some(override_dir) = std::env::var_os(INDEX_DIR_OVERRIDE_ENV) {
        return PathBuf::from(override_dir);
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        #[cfg(target_os = "macos")]
        {
            return home.join("Library").join("Application Support");
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                return PathBuf::from(local);
            }
            return home.join("AppData").join("Local");
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            return home.join(".local").join("share");
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local);
    }
    std::env::temp_dir()
}

/// Where a repo's index lives under `base`: `<base>/agentloop/index/<repo-hash>`
/// — in normal operation `base` is the platform app-data dir
/// ([`index_root_base`]), so this resolves to e.g.
/// `~/Library/Application Support/agentloop/index/<repo-hash>` on macOS —
/// **never** inside the repo being indexed. Keying by a blake3 hash of the
/// canonicalized repo root means every worktree/session for the same repo
/// shares one on-disk index (per the scanner's design: incrementality keys
/// on blob hash, not path), while distinct repos never collide. The crate
/// itself stays agnostic about location: `IndexStore::open` takes an
/// explicit `index_dir`; this function is the one policy decision, made
/// once here for every tool that needs "the index for this cwd".
pub fn index_dir_for(cwd: &Path, base: &Path) -> PathBuf {
    let repo_root = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let hash = blake3::hash(repo_root.to_string_lossy().as_bytes());
    let repo_hash = hash.to_hex();

    base.join("agentloop")
        .join("index")
        .join(repo_hash.as_str())
}

/// Whether a tool/hook should refresh the on-disk index before searching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndexOpenMode {
    /// Always scan the repo and incrementally update changed files.
    AutoUpdate,
    /// Reuse a non-empty on-disk index without scanning. Builds only when
    /// the index is missing or empty (first use). Default for desktop so a
    /// warm index is not re-scanned on every new chat's `RepoMap` call.
    #[default]
    ReuseWarm,
}

impl IndexOpenMode {
    pub fn from_auto_update(auto_update: bool) -> Self {
        if auto_update {
            Self::AutoUpdate
        } else {
            Self::ReuseWarm
        }
    }
}

/// Open (or create) the index rooted at `cwd` and optionally bring it up to
/// date, at the given `index_dir`. The actual worker behind [`open_and_build`];
/// split out so tests can pass an explicit scratch `index_dir` instead of
/// depending on [`index_root_base`]'s env/`$HOME` resolution (this workspace
/// forbids `unsafe_code`, so tests can't scope an env-var override the way
/// `packages/desktop`'s composition root does elsewhere in this repo).
///
/// When `events` is provided and a build runs, emits `IndexingStarted` /
/// progress notes / `IndexingCompleted` so the chat UI can show "Indexing
/// repository…" instead of a silent hang. Progress notes use `ToolProgress`
/// when `call_id` is set.
///
/// Prefer [`crate::status_for`] / [`IndexStore::status_counts`] for readiness
/// checks — those never open tantivy.
fn open_and_build_at(
    cwd: &Path,
    index_dir: &Path,
    events: Option<&EventSink>,
    call_id: Option<&ToolCallId>,
    mode: IndexOpenMode,
) -> Result<IndexStore, ToolError> {
    let embedder = resolve_embedder(index_dir).map_err(|err| {
        ToolError::Execution(format!(
            "Could not initialize embeddings for the code index: {err}."
        ))
    })?;
    let mut store = match embedder {
        Some(provider) => IndexStore::open_with_embeddings(cwd, index_dir, provider),
        None => IndexStore::open(cwd, index_dir),
    }
    .map_err(|err| {
        ToolError::Execution(format!(
            "Could not open the code index at `{}`: {err}.",
            index_dir.display()
        ))
    })?;

    let was_empty = store.indexed_file_count() == 0;
    if !was_empty && mode == IndexOpenMode::ReuseWarm {
        tracing::debug!(
            cwd = %cwd.display(),
            files = store.indexed_file_count(),
            "SearchCode/FindSymbol: reusing warm code index (auto-update off)"
        );
        return Ok(store);
    }

    if was_empty {
        tracing::info!(cwd = %cwd.display(), "SearchCode/FindSymbol: building code index for the first time");
        if let Some(sink) = events {
            sink.emit(AgentEvent::IndexingStarted {
                reason: "first_build".to_owned(),
            });
            if let Some(id) = call_id {
                sink.emit(AgentEvent::ToolProgress {
                    call_id: id.clone(),
                    note: "Building code index for the first time…".to_owned(),
                });
            }
        }
    }

    let mut announced_update = false;
    let stats: UpdateStats = store
        .build_with_progress(|done, total| {
            if let Some(sink) = events {
                if !was_empty && !announced_update && total > 0 {
                    announced_update = true;
                    sink.emit(AgentEvent::IndexingStarted {
                        reason: "update".to_owned(),
                    });
                }
                if total > 0 {
                    let note = if was_empty {
                        format!("Indexing repository… {done}/{total} files")
                    } else {
                        format!("Updating code index… {done}/{total} files")
                    };
                    if let Some(id) = call_id {
                        sink.emit(AgentEvent::ToolProgress {
                            call_id: id.clone(),
                            note,
                        });
                    }
                }
            }
        })
        .map_err(|err| ToolError::Execution(format!("Failed to build the code index: {err}.")))?;

    let total = stats.added + stats.changed + stats.unchanged;
    let did_work = was_empty || stats.added + stats.changed > 0 || announced_update;
    if did_work {
        if let Some(sink) = events {
            sink.emit(AgentEvent::IndexingCompleted {
                added: stats.added as u32,
                changed: stats.changed as u32,
                removed: stats.removed as u32,
                unchanged: stats.unchanged as u32,
            });
            if let Some(id) = call_id {
                let indexed = stats.added + stats.changed + stats.unchanged;
                sink.emit(AgentEvent::ToolProgress {
                    call_id: id.clone(),
                    note: format!("Indexed {indexed} files"),
                });
            }
        }
    }

    if was_empty && total > LARGE_REPO_FILE_THRESHOLD {
        tracing::info!(
            added = stats.added,
            changed = stats.changed,
            unchanged = stats.unchanged,
            removed = stats.removed,
            "SearchCode/FindSymbol: indexed a large repo"
        );
    }

    Ok(store)
}

/// Open (or create) the index rooted at `cwd` and bring it up to date.
pub fn open_and_build(cwd: &Path) -> Result<IndexStore, ToolError> {
    open_and_build_with_mode(cwd, IndexOpenMode::AutoUpdate)
}

/// Open the index for `cwd` under [`IndexOpenMode`].
pub fn open_and_build_with_mode(cwd: &Path, mode: IndexOpenMode) -> Result<IndexStore, ToolError> {
    let index_dir = index_dir_for(cwd, &index_root_base());
    open_and_build_at(cwd, &index_dir, None, None, mode)
}

/// Like [`open_and_build`], but streams indexing status into `events` so the
/// chat UI can show live "Indexing repository…" feedback.
pub fn open_and_build_with_events(
    cwd: &Path,
    events: &EventSink,
    call_id: Option<&ToolCallId>,
) -> Result<IndexStore, ToolError> {
    open_and_build_with_events_mode(cwd, events, call_id, IndexOpenMode::AutoUpdate)
}

/// Like [`open_and_build_with_events`], with an explicit [`IndexOpenMode`].
pub fn open_and_build_with_events_mode(
    cwd: &Path,
    events: &EventSink,
    call_id: Option<&ToolCallId>,
    mode: IndexOpenMode,
) -> Result<IndexStore, ToolError> {
    let index_dir = index_dir_for(cwd, &index_root_base());
    open_and_build_at(cwd, &index_dir, Some(events), call_id, mode)
}

/// Test-only equivalent of [`open_and_build`] that indexes under an explicit
/// scratch directory (a `tempfile::TempDir`) instead of the real app-data
/// index root, so test runs never write outside their own tempdirs.
#[cfg(test)]
pub(crate) fn open_and_build_in(cwd: &Path, index_root: &Path) -> Result<IndexStore, ToolError> {
    open_and_build_in_mode(cwd, index_root, IndexOpenMode::AutoUpdate)
}

/// Test-only open under an explicit index root and [`IndexOpenMode`].
#[cfg(test)]
pub(crate) fn open_and_build_in_mode(
    cwd: &Path,
    index_root: &Path,
    mode: IndexOpenMode,
) -> Result<IndexStore, ToolError> {
    let index_dir = index_dir_for(cwd, index_root);
    open_and_build_at(cwd, &index_dir, None, None, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_build_indexes_then_picks_up_new_files_on_next_call() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        std::fs::write(repo.path().join("a.rs"), "fn a() {}").unwrap_or_else(|e| panic!("{e}"));

        let store =
            open_and_build_in(repo.path(), index_root.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(store.indexed_file_count(), 1);

        std::fs::write(repo.path().join("b.rs"), "fn b() {}").unwrap_or_else(|e| panic!("{e}"));
        let store2 =
            open_and_build_in(repo.path(), index_root.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(store2.indexed_file_count(), 2);
    }

    #[test]
    fn reuse_warm_skips_update_when_index_already_populated() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        std::fs::write(repo.path().join("a.rs"), "fn a() {}").unwrap_or_else(|e| panic!("{e}"));

        let store =
            open_and_build_in(repo.path(), index_root.path()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(store.indexed_file_count(), 1);

        std::fs::write(repo.path().join("b.rs"), "fn b() {}").unwrap_or_else(|e| panic!("{e}"));
        let warm = open_and_build_in_mode(repo.path(), index_root.path(), IndexOpenMode::ReuseWarm)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            warm.indexed_file_count(),
            1,
            "ReuseWarm must not pick up new files without an explicit rebuild"
        );

        let updated =
            open_and_build_in_mode(repo.path(), index_root.path(), IndexOpenMode::AutoUpdate)
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(updated.indexed_file_count(), 2);
    }

    #[test]
    fn index_dir_for_never_uses_repo_root() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let repo = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let dir = index_dir_for(repo.path(), index_root.path());
        assert!(
            !dir.starts_with(repo.path()),
            "index dir {dir:?} must not live inside the repo root {:?}",
            repo.path()
        );
        assert!(
            dir.ends_with("index") || dir.parent().is_some_and(|p| p.ends_with("index")),
            "expected …/agentloop/index/<hash>, got {dir:?}"
        );
        assert!(
            dir.to_string_lossy().contains("agentloop"),
            "expected agentloop segment in {dir:?}"
        );
    }

    #[test]
    fn index_root_base_is_outside_typical_repo_paths() {
        let base = index_root_base();
        // Never the process cwd (a repo checkout in normal use) and never a
        // bare `$HOME` — it must be an app-data / override / temp location.
        assert_ne!(base, PathBuf::from("."));
        if let Some(home) = std::env::var_os("HOME") {
            assert_ne!(base, PathBuf::from(home));
        }
    }

    #[test]
    fn index_root_override_redirects_without_env_mutation() {
        let scratch = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        set_index_root_override(Some(scratch.path().to_path_buf()));
        assert_eq!(index_root_base(), scratch.path());
        set_index_root_override(None);
        assert_ne!(index_root_base(), scratch.path());
    }
}
