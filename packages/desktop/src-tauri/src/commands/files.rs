//! Workspace file search, tree listing, and text file CRUD.

use super::common::{
    normalize_path_slashes, require_service, resolve_review_path, validate_repo_relative_path,
};
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    /// Path relative to `cwd`, forward-slashed.
    pub path: String,
    /// Basename, shown as the primary label.
    pub name: String,
    /// True when the hit is a directory (folder icon in @-mentions / Files).
    #[serde(default)]
    pub is_dir: bool,
}

/// Directory basenames we never descend into, even if a project forgot to
/// gitignore them — walking `node_modules` / build outputs is what made
/// composer `@` feel multi-second on ordinary apps.
const SKIP_DIR_NAMES: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".output",
    ".turbo",
    ".cache",
    ".parcel-cache",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    "vendor",
    "Pods",
    ".svelte-kit",
    ".vercel",
    ".idea",
    ".vscode",
];

pub(crate) fn is_skipped_dir_name(name: &str) -> bool {
    SKIP_DIR_NAMES
        .iter()
        .any(|skip| name.eq_ignore_ascii_case(skip))
}

/// Rank a path against a lowercase needle. Lower is better; `None` = no match.
/// Basename prefix/contains beat full-path contains. Subsequence matching is
/// intentionally omitted — it matched almost everything and made ranking +
/// result lists feel random/laggy.
pub(crate) fn score_file(rel_path: &str, name: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(100); // browse mode — rank by path length afterwards
    }
    let path_l = rel_path.to_lowercase();
    let name_l = name.to_lowercase();
    if name_l.starts_with(needle) {
        return Some(0);
    }
    if name_l.contains(needle) {
        return Some(1);
    }
    if path_l.contains(needle) {
        return Some(2);
    }
    None
}

/// Read-only fuzzy file **and folder** search under `cwd` for composer
/// @-mentions and the Files explorer search box. Folders are included so the
/// suggestion tray can show distinct file/folder icons.
///
/// By default (`include_ignored = false` / omitted): respects `.gitignore` /
/// `.ignore` / `.git/exclude` and skips hidden files — correct for agent
/// @-mentions. Pass `include_ignored = true` for the human Files search so
/// `.env` and other gitignored paths are findable (heavy vendor dirs are still
/// pruned so typing stays snappy).
///
/// Walks once per `(root, include_ignored)` into
/// [`AppState::workspace_path_cache`]; subsequent keystrokes score against
/// the warm list until TTL expiry or explicit invalidation.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub async fn list_files(
    state: State<'_, AppState>,
    cwd: String,
    query: String,
    include_ignored: Option<bool>,
    fallback_cwd: Option<String>,
) -> DesktopResult<Vec<FileHit>> {
    let Some(root) = crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
    else {
        return Ok(Vec::new());
    };
    let needle = query.trim().to_lowercase();
    let include_ignored = include_ignored.unwrap_or(false);
    let root_key = root.to_string_lossy().to_string();
    let cache_key = (root_key.clone(), include_ignored);

    // Fast path: reuse a fresh warm walk without leaving the async worker.
    {
        let cache = state
            .workspace_path_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = cache.get(&cache_key) {
            if entry.fresh() {
                return Ok(score_cached_paths(&entry.entries, &needle));
            }
        }
    }

    let root_for_walk = root.clone();
    let entries =
        tokio::task::spawn_blocking(move || walk_workspace_paths(&root_for_walk, include_ignored))
            .await
            .map_err(|err| DesktopError::Message(format!("list_files walk join: {err}")))?;

    {
        let mut cache = state
            .workspace_path_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        cache.insert(
            cache_key,
            crate::state::WorkspacePathCache {
                entries: entries.clone(),
                built_at: std::time::Instant::now(),
            },
        );
    }

    Ok(score_cached_paths(&entries, &needle))
}

/// Drop the warm `list_files` path cache. Pass `cwd` to clear only entries
/// for that workspace root; omit to clear everything (used after FS-mutating
/// tools / turn settle).
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn invalidate_workspace_path_cache(state: State<'_, AppState>, cwd: Option<String>) {
    let mut cache = state
        .workspace_path_cache
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    match cwd.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(cwd) => {
            let resolved = crate::path_resolve::resolve_existing_dir(cwd, None)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| cwd.to_string());
            cache.retain(|(root, _), _| root != &resolved && !root.starts_with(&resolved));
            // Also drop keys that were stored before canonicalize (raw cwd).
            cache.retain(|(root, _), _| root != cwd);
        }
        None => cache.clear(),
    }
}

/// Bound the walk so huge repos can't stall an interactive keystroke.
const MAX_HITS: usize = 40;
const MAX_WALK_SEARCH: usize = 8_000;

pub(crate) fn walk_workspace_paths(
    root: &std::path::Path,
    include_ignored: bool,
) -> Vec<crate::state::CachedPathEntry> {
    let mut entries: Vec<crate::state::CachedPathEntry> = Vec::new();
    let mut walked = 0usize;

    let mut builder = ignore::WalkBuilder::new(root);
    if include_ignored {
        // Human Files search: show hidden + gitignored (e.g. `.env`).
        builder
            .standard_filters(false)
            .hidden(false)
            .git_ignore(false)
            .git_global(false)
            .git_exclude(false)
            .ignore(false)
            .parents(false)
            .follow_links(false);
    } else {
        builder
            .standard_filters(true) // hidden + gitignore + .ignore + exclude
            .parents(true)
            .follow_links(false);
    }
    builder.filter_entry(|entry| {
        // Always prune heavy vendor dirs before descending — otherwise a
        // missing ignore rule (or include_ignored) walks tens of thousands
        // of node_modules files on every keystroke.
        if entry.depth() > 0 && entry.file_type().is_some_and(|ft| ft.is_dir()) {
            let name = entry.file_name().to_string_lossy();
            if is_skipped_dir_name(&name) {
                return false;
            }
        }
        true
    });

    for entry in builder.build().flatten() {
        walked += 1;
        if walked > MAX_WALK_SEARCH {
            break;
        }
        let Some(ft) = entry.file_type() else {
            continue;
        };
        let is_dir = ft.is_dir();
        let is_file = ft.is_file();
        if !is_dir && !is_file {
            continue;
        }
        // Skip the walk root itself.
        if entry.depth() == 0 {
            continue;
        }
        let entry_path = entry.path();
        let Ok(rel) = entry_path.strip_prefix(root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.is_empty() {
            continue;
        }
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| rel_str.clone());
        entries.push(crate::state::CachedPathEntry {
            path: rel_str,
            name,
            is_dir,
        });
    }
    entries
}

pub(crate) fn score_cached_paths(
    entries: &[crate::state::CachedPathEntry],
    needle: &str,
) -> Vec<FileHit> {
    // Warm-cache scoring is cheap (string checks only) — score the full list
    // then rank, instead of the walk-time early-exit that stopped disk I/O.
    let mut hits: Vec<(i32, FileHit)> = Vec::with_capacity(MAX_HITS * 2);

    for entry in entries {
        let Some(score) = score_file(&entry.path, &entry.name, needle) else {
            continue;
        };
        // Prefer files slightly over folders when scores tie (files more often
        // what users @-mention), but still surface folders with folder icons.
        let rank = if entry.is_dir { score + 10 } else { score };
        hits.push((
            rank,
            FileHit {
                path: entry.path.clone(),
                name: entry.name.clone(),
                is_dir: entry.is_dir,
            },
        ));
    }

    hits.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.path.len().cmp(&b.1.path.len()))
            .then_with(|| a.1.path.cmp(&b.1.path))
    });
    hits.truncate(MAX_HITS);
    hits.into_iter().map(|(_, h)| h).collect()
}

/// Resolve a usable workspace directory from `cwd`, falling back to
/// `fallback_cwd` (typically `base_cwd`) when the primary path is missing.
/// Returns the absolute/normalized path string, or `None` when neither exists.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn resolve_workspace_cwd(cwd: String, fallback_cwd: Option<String>) -> Option<String> {
    crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Immediate children of `relative_dir` under `cwd` (empty = workspace root).
/// Human Files tree: shows **everything** at this level — including `.env`,
/// gitignored paths, and heavy dirs like `node_modules` — so the panel matches
/// what a person sees in Explorer/Finder. (Composer `@` still uses
/// gitignore-aware [`list_files`].) Soft-capped; dirs-first then name.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub async fn list_dir_children(
    cwd: String,
    relative_dir: String,
    fallback_cwd: Option<String>,
) -> Vec<FileHit> {
    tokio::task::spawn_blocking(move || list_dir_children_sync(&cwd, &relative_dir, fallback_cwd))
        .await
        .unwrap_or_default()
}

pub(crate) fn list_dir_children_sync(
    cwd: &str,
    relative_dir: &str,
    fallback_cwd: Option<String>,
) -> Vec<FileHit> {
    let Some(root) = crate::path_resolve::resolve_existing_dir(cwd, fallback_cwd.as_deref()) else {
        return Vec::new();
    };

    let rel = relative_dir.trim().trim_matches('/').replace('\\', "/");
    if rel.contains("..") {
        return Vec::new();
    }
    if !rel.is_empty() && validate_repo_relative_path(&rel).is_err() {
        return Vec::new();
    }

    let dir = if rel.is_empty() {
        root.clone()
    } else {
        root.join(&rel)
    };
    if !dir.is_dir() {
        return Vec::new();
    }

    const MAX_CHILDREN: usize = 1_000;
    let mut hits: Vec<FileHit> = Vec::new();

    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    for entry in read.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let is_dir = meta.is_dir();
        if !is_dir && !meta.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "." || name == ".." {
            continue;
        }
        let entry_path = entry.path();
        let Ok(stripped) = entry_path.strip_prefix(&root) else {
            continue;
        };
        let path = normalize_path_slashes(&stripped.to_string_lossy());
        if path.is_empty() {
            continue;
        }
        hits.push(FileHit { path, name, is_dir });
        if hits.len() >= MAX_CHILDREN {
            break;
        }
    }

    hits.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.path.cmp(&b.path))
    });
    hits
}

#[cfg(test)]
mod list_files_ranking_tests {
    use super::{is_skipped_dir_name, score_file};

    #[test]
    fn skips_heavy_vendor_dirs() {
        assert!(is_skipped_dir_name("node_modules"));
        assert!(is_skipped_dir_name("NODE_MODULES"));
        assert!(is_skipped_dir_name(".git"));
        assert!(is_skipped_dir_name("target"));
        assert!(!is_skipped_dir_name("src"));
    }

    #[test]
    fn scores_basename_prefix_best() {
        assert_eq!(score_file("pkg/App.tsx", "App.tsx", "app"), Some(0));
        assert_eq!(score_file("pkg/MyApp.tsx", "MyApp.tsx", "app"), Some(1));
        assert_eq!(score_file("app/page.tsx", "page.tsx", "app"), Some(2));
        assert_eq!(score_file("pkg/page.tsx", "page.tsx", "zzz"), None);
    }

    #[test]
    fn empty_needle_is_browse_mode() {
        assert_eq!(score_file("a.ts", "a.ts", ""), Some(100));
    }
}

// ---------------------------------------------------------------------------
// Plan tab: Save to Workspace — writes the rendered plan markdown to a file
// inside the session's cwd. Traversal-hardened: canonicalize the cwd, join
// the (resolved) relative path, then verify the written file's parent still
// sits inside the canonical cwd. Absolute Write/Edit-style paths are
// accepted via [`resolve_review_path`] and stripped to repo-relative first.
// ---------------------------------------------------------------------------

/// Hard cap for the Files (Monaco) editor — keeps the UI responsive and
/// rejects accidental binary dumps. Matches ~ Cursor's soft ceiling for
/// opening source in the side editor.
const READ_TEXT_MAX_BYTES: u64 = 1_500_000;

/// Reads a UTF-8 text file relative to `session_id`'s cwd. Same path
/// sanitation as [`save_text_file`]. Rejects files larger than
/// [`READ_TEXT_MAX_BYTES`] and non-UTF-8 content.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn read_text_file(
    state: State<'_, AppState>,
    session_id: String,
    relative_path: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;

    let relative = resolve_review_path(&relative_path, &meta.cwd)?;
    let cwd = meta.cwd.clone();

    tokio::task::spawn_blocking(move || {
        let canonical_cwd = cwd
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;
        let target = canonical_cwd.join(&relative);

        let canonical_target = target.canonicalize().map_err(|e| {
            DesktopError::Message(format!("cannot open `{}`: {e}", target.display()))
        })?;
        if !canonical_target.starts_with(&canonical_cwd) {
            return Err(DesktopError::Message(
                "resolved path escapes the session's working directory".into(),
            ));
        }
        if !canonical_target.is_file() {
            return Err(DesktopError::Message(format!(
                "`{}` is not a file",
                relative
            )));
        }

        let meta = std::fs::metadata(&canonical_target)
            .map_err(|e| DesktopError::Message(format!("cannot stat `{}`: {e}", relative)))?;
        if meta.len() > READ_TEXT_MAX_BYTES {
            return Err(DesktopError::Message(format!(
                "`{relative}` is too large to open in the editor ({} bytes, max {})",
                meta.len(),
                READ_TEXT_MAX_BYTES
            )));
        }

        let bytes = std::fs::read(&canonical_target)
            .map_err(|e| DesktopError::Message(format!("cannot read `{relative}`: {e}")))?;
        if bytes.contains(&0) {
            return Err(DesktopError::Message(format!(
                "`{relative}` looks binary — open it in an external editor"
            )));
        }
        String::from_utf8(bytes).map_err(|_| {
            DesktopError::Message(format!(
                "`{relative}` is not valid UTF-8 — open it in an external editor"
            ))
        })
    })
    .await
    .map_err(|e| DesktopError::Message(format!("read join: {e}")))?
}

/// Writes `content` to `relative_path` inside `session_id`'s cwd, creating
/// parent directories as needed (e.g. a not-yet-existing `plans/` folder).
/// Returns the absolute path written. Used by the Plan tab's "Save to
/// Workspace" menu item (`PlanToolbar`); the frontend passes
/// `plans/<slug>-<date>.md`.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn save_text_file(
    state: State<'_, AppState>,
    session_id: String,
    relative_path: String,
    content: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;

    let relative = resolve_review_path(&relative_path, &meta.cwd)?;

    let canonical_cwd = meta
        .cwd
        .canonicalize()
        .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;

    let target = canonical_cwd.join(&relative);
    let parent = target
        .parent()
        .ok_or_else(|| DesktopError::Message("path has no parent directory".into()))?;

    std::fs::create_dir_all(parent)
        .map_err(|e| DesktopError::Message(format!("cannot create `{}`: {e}", parent.display())))?;

    // Re-canonicalize the parent now that it's guaranteed to exist, and
    // verify it's still inside the session cwd — belt-and-suspenders against
    // `..` traversal via symlinks that the component scan above can't see.
    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| DesktopError::Message(format!("invalid target directory: {e}")))?;
    if !canonical_parent.starts_with(&canonical_cwd) {
        return Err(DesktopError::Message(
            "resolved path escapes the session's working directory".into(),
        ));
    }

    std::fs::write(&target, content)
        .map_err(|e| DesktopError::Message(format!("cannot write `{}`: {e}", target.display())))?;

    Ok(target.display().to_string())
}

/// Creates an empty UTF-8 text file at `relative_path` inside `session_id`'s
/// cwd (creating parent dirs as needed). Fails if the target already exists —
/// use [`save_text_file`] to overwrite. Returns the repo-relative path.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn create_text_file(
    state: State<'_, AppState>,
    session_id: String,
    relative_path: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;

    let relative = resolve_review_path(&relative_path, &meta.cwd)?;
    let cwd = meta.cwd.clone();

    tokio::task::spawn_blocking(move || {
        let canonical_cwd = cwd
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;
        let target = canonical_cwd.join(&relative);

        if target.exists() {
            return Err(DesktopError::Message(format!(
                "`{relative}` already exists"
            )));
        }

        let parent = target
            .parent()
            .ok_or_else(|| DesktopError::Message("path has no parent directory".into()))?;
        std::fs::create_dir_all(parent).map_err(|e| {
            DesktopError::Message(format!("cannot create `{}`: {e}", parent.display()))
        })?;

        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("invalid target directory: {e}")))?;
        if !canonical_parent.starts_with(&canonical_cwd) {
            return Err(DesktopError::Message(
                "resolved path escapes the session's working directory".into(),
            ));
        }

        std::fs::write(&target, "").map_err(|e| {
            DesktopError::Message(format!("cannot create `{}`: {e}", target.display()))
        })?;
        Ok(relative)
    })
    .await
    .map_err(|e| DesktopError::Message(format!("create join: {e}")))?
}

/// Renames a file under `session_id`'s cwd from `from_path` to `to_path`
/// (both repo-relative). Fails if the destination already exists, if the
/// source is missing, or if either path escapes the session cwd. Returns the
/// new repo-relative path.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn rename_path(
    state: State<'_, AppState>,
    session_id: String,
    from_path: String,
    to_path: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;

    let from_rel = resolve_review_path(&from_path, &meta.cwd)?;
    let to_rel = resolve_review_path(&to_path, &meta.cwd)?;
    if from_rel == to_rel {
        return Ok(to_rel);
    }
    let cwd = meta.cwd.clone();

    tokio::task::spawn_blocking(move || {
        let canonical_cwd = cwd
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;
        let from = canonical_cwd.join(&from_rel);
        let to = canonical_cwd.join(&to_rel);

        let canonical_from = from
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("cannot rename `{}`: {e}", from_rel)))?;
        if !canonical_from.starts_with(&canonical_cwd) {
            return Err(DesktopError::Message(
                "source path escapes the session's working directory".into(),
            ));
        }
        if !canonical_from.is_file() {
            return Err(DesktopError::Message(format!("`{from_rel}` is not a file")));
        }

        if to.exists() {
            return Err(DesktopError::Message(format!("`{to_rel}` already exists")));
        }

        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DesktopError::Message(format!("cannot create `{}`: {e}", parent.display()))
            })?;
            let canonical_parent = parent
                .canonicalize()
                .map_err(|e| DesktopError::Message(format!("invalid target directory: {e}")))?;
            if !canonical_parent.starts_with(&canonical_cwd) {
                return Err(DesktopError::Message(
                    "destination path escapes the session's working directory".into(),
                ));
            }
        }

        std::fs::rename(&canonical_from, &to).map_err(|e| {
            DesktopError::Message(format!("cannot rename `{from_rel}` → `{to_rel}`: {e}"))
        })?;
        Ok(to_rel)
    })
    .await
    .map_err(|e| DesktopError::Message(format!("rename join: {e}")))?
}

/// Deletes a file under `session_id`'s cwd. Refuses directories (explorer is
/// file-scoped). Returns the deleted repo-relative path.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn delete_path(
    state: State<'_, AppState>,
    session_id: String,
    relative_path: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;

    let relative = resolve_review_path(&relative_path, &meta.cwd)?;
    let cwd = meta.cwd.clone();

    tokio::task::spawn_blocking(move || {
        let canonical_cwd = cwd
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("invalid session cwd: {e}")))?;
        let target = canonical_cwd.join(&relative);

        let canonical_target = target
            .canonicalize()
            .map_err(|e| DesktopError::Message(format!("cannot delete `{}`: {e}", relative)))?;
        if !canonical_target.starts_with(&canonical_cwd) {
            return Err(DesktopError::Message(
                "resolved path escapes the session's working directory".into(),
            ));
        }
        if !canonical_target.is_file() {
            return Err(DesktopError::Message(format!("`{relative}` is not a file")));
        }

        std::fs::remove_file(&canonical_target)
            .map_err(|e| DesktopError::Message(format!("cannot delete `{relative}`: {e}")))?;
        Ok(relative)
    })
    .await
    .map_err(|e| DesktopError::Message(format!("delete join: {e}")))?
}
