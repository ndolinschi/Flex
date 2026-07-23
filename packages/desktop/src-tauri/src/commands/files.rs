
use super::common::{
    normalize_path_slashes, require_service, resolve_review_path, validate_repo_relative_path,
};
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub is_dir: bool,
}

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

pub(crate) fn score_file(rel_path: &str, name: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(100);
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
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from(cwd));
            cache.retain(|(root, _), _| {
                let root_path = std::path::Path::new(root);
                root_path != resolved.as_path() && !root_path.starts_with(&resolved)
            });
            cache.retain(|(root, _), _| root != cwd);
        }
        None => cache.clear(),
    }
}

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
            .standard_filters(true)
            .parents(true)
            .follow_links(false);
    }
    builder.filter_entry(|entry| {
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
    let mut hits: Vec<(i32, FileHit)> = Vec::with_capacity(MAX_HITS * 2);

    for entry in entries {
        let Some(score) = score_file(&entry.path, &entry.name, needle) else {
            continue;
        };
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

#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn resolve_workspace_cwd(cwd: String, fallback_cwd: Option<String>) -> Option<String> {
    crate::path_resolve::resolve_existing_dir(&cwd, fallback_cwd.as_deref())
        .map(|p| p.to_string_lossy().into_owned())
}

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

const READ_TEXT_MAX_BYTES: u64 = 1_500_000;

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
