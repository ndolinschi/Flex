use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use agentloop_contracts::{AgentEvent, ToolCallId};
use agentloop_core::{EventSink, ToolError};

use crate::embed::resolve_embedder;
use crate::store::{IndexStore, UpdateStats};

const LARGE_REPO_FILE_THRESHOLD: usize = 20_000;

pub(crate) const INDEX_DIR_OVERRIDE_ENV: &str = "AGENTLOOP_INDEX_DIR";

static INDEX_ROOT_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub const AUTO_UPDATE_ENV: &str = "AGENTLOOP_INDEX_AUTO_UPDATE";

pub fn env_auto_update_enabled() -> bool {
    match std::env::var(AUTO_UPDATE_ENV) {
        Ok(raw) => {
            let v = raw.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "on" | "yes")
        }
        Err(_) => false,
    }
}

#[cfg(test)]
static INDEX_ROOT_OVERRIDE_GATE: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn set_index_root_override(path: Option<PathBuf>) {
    let mut guard = INDEX_ROOT_OVERRIDE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = path;
}

#[cfg(test)]
pub(crate) fn lock_index_root_override() -> std::sync::MutexGuard<'static, ()> {
    INDEX_ROOT_OVERRIDE_GATE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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

pub fn index_dir_for(cwd: &Path, base: &Path) -> PathBuf {
    let repo_root = index_identity_root(cwd);
    let hash = blake3::hash(repo_root.to_string_lossy().as_bytes());
    let repo_hash = hash.to_hex();

    base.join("agentloop")
        .join("index")
        .join(repo_hash.as_str())
}

pub(crate) fn index_identity_root(cwd: &Path) -> PathBuf {
    let canon = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    if let Some(main) = git_main_worktree_root(&canon) {
        return main;
    }
    canon
}

fn git_main_worktree_root(cwd: &Path) -> Option<PathBuf> {
    for dir in cwd.ancestors() {
        let git = dir.join(".git");
        if git.is_dir() {
            return Some(dir.to_path_buf());
        }
        if !git.is_file() {
            continue;
        }
        let contents = fs::read_to_string(&git).ok()?;
        let gitdir = contents
            .lines()
            .find_map(|line| {
                line.strip_prefix("gitdir:")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            })?
            .to_owned();
        let mut gitdir_path = PathBuf::from(&gitdir);
        if gitdir_path.is_relative() {
            gitdir_path = dir.join(gitdir_path);
        }
        let gitdir_path = gitdir_path.canonicalize().unwrap_or(gitdir_path);
        for ancestor in gitdir_path.ancestors() {
            if ancestor.file_name().is_some_and(|n| n == ".git") {
                return ancestor.parent().map(Path::to_path_buf);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndexOpenMode {
    AutoUpdate,
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

    let mut announced = false;
    if was_empty {
        tracing::info!(cwd = %cwd.display(), "SearchCode/FindSymbol: building code index for the first time");
        if let Some(sink) = events {
            announced = true;
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
    let stats: UpdateStats = match store.build_with_progress(|done, total| {
        if let Some(sink) = events {
            if !was_empty && !announced_update && total > 0 {
                announced_update = true;
                announced = true;
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
    }) {
        Ok(stats) => stats,
        Err(err) => {
            if announced {
                if let Some(sink) = events {
                    sink.emit(AgentEvent::IndexingCompleted {
                        added: 0,
                        changed: 0,
                        removed: 0,
                        unchanged: 0,
                    });
                }
            }
            return Err(ToolError::Execution(format!(
                "Failed to build the code index: {err}."
            )));
        }
    };

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

pub fn open_and_build(cwd: &Path) -> Result<IndexStore, ToolError> {
    open_and_build_with_mode(cwd, IndexOpenMode::AutoUpdate)
}

pub fn open_and_build_with_mode(cwd: &Path, mode: IndexOpenMode) -> Result<IndexStore, ToolError> {
    let index_dir = index_dir_for(cwd, &index_root_base());
    open_and_build_at(cwd, &index_dir, None, None, mode)
}

pub fn open_and_build_with_events(
    cwd: &Path,
    events: &EventSink,
    call_id: Option<&ToolCallId>,
) -> Result<IndexStore, ToolError> {
    open_and_build_with_events_mode(cwd, events, call_id, IndexOpenMode::AutoUpdate)
}

pub fn open_and_build_with_events_mode(
    cwd: &Path,
    events: &EventSink,
    call_id: Option<&ToolCallId>,
    mode: IndexOpenMode,
) -> Result<IndexStore, ToolError> {
    let index_dir = index_dir_for(cwd, &index_root_base());
    open_and_build_at(cwd, &index_dir, Some(events), call_id, mode)
}

#[cfg(test)]
pub(crate) fn open_and_build_in(cwd: &Path, index_root: &Path) -> Result<IndexStore, ToolError> {
    open_and_build_in_mode(cwd, index_root, IndexOpenMode::AutoUpdate)
}

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
    fn index_dir_for_shares_identity_across_linked_worktree() {
        let index_root = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let main = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let worktree = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let git_dir = main.path().join(".git");
        fs::create_dir_all(git_dir.join("worktrees").join("wt")).unwrap_or_else(|e| panic!("{e}"));
        let gitdir_line = format!(
            "gitdir: {}\n",
            git_dir.join("worktrees").join("wt").display()
        );
        fs::write(worktree.path().join(".git"), gitdir_line).unwrap_or_else(|e| panic!("{e}"));

        let main_dir = index_dir_for(main.path(), index_root.path());
        let wt_dir = index_dir_for(worktree.path(), index_root.path());
        assert_eq!(
            main_dir, wt_dir,
            "linked worktree must share the main repo's index dir"
        );
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
