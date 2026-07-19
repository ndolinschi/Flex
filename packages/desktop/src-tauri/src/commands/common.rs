//! Shared helpers used across command modules.

use super::prelude::*;

pub(crate) async fn require_service(state: &AppState) -> DesktopResult<EngineService> {
    state
        .service
        .lock()
        .await
        .clone()
        .ok_or(DesktopError::NotConfigured)
}

pub(crate) fn parse_isolation(raw: Option<&str>) -> Option<IsolationPolicy> {
    match raw? {
        "never" => Some(IsolationPolicy::Never),
        "optional" => Some(IsolationPolicy::Optional),
        "required" => Some(IsolationPolicy::Required),
        _ => None,
    }
}

/// Reject absolute paths and any path containing a `..` component — every
/// review command takes a path that is supposed to be repo-relative, and
/// these are shelled straight into `git -C <dir> ... -- <path>` /
/// filesystem calls, so path traversal must be ruled out up front.
pub(crate) fn validate_repo_relative_path(path: &str) -> DesktopResult<&str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("path is required".into()));
    }
    let as_path = std::path::Path::new(trimmed);
    if as_path.is_absolute() {
        return Err(DesktopError::Message(format!(
            "path must be repo-relative, got absolute path: {trimmed}"
        )));
    }
    if as_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(DesktopError::Message(format!(
            "path must not contain '..': {trimmed}"
        )));
    }
    Ok(trimmed)
}

pub(crate) fn normalize_path_slashes(s: &str) -> String {
    s.replace('\\', "/")
}

/// Strip `worktree` from an absolute `path`, returning a forward-slashed
/// relative path. Lexical first (so deleted files still resolve), then
/// canonicalize when both sides exist (symlinks / Windows drive casing).
pub(crate) fn strip_worktree_prefix(
    path: &std::path::Path,
    worktree: &std::path::Path,
) -> DesktopResult<PathBuf> {
    if let Ok(rel) = path.strip_prefix(worktree) {
        return Ok(rel.to_path_buf());
    }

    let path_s = normalize_path_slashes(&path.to_string_lossy());
    let mut root_s = normalize_path_slashes(&worktree.to_string_lossy());
    while root_s.ends_with('/') {
        root_s.pop();
    }
    if path_s == root_s {
        return Ok(PathBuf::new());
    }
    let prefix = format!("{root_s}/");
    if let Some(rest) = path_s.strip_prefix(&prefix) {
        return Ok(PathBuf::from(rest));
    }
    // Windows FS is case-insensitive — tool `file_path`s and SessionMeta.cwd
    // often disagree on drive-letter casing (`C:\` vs `c:\`).
    #[cfg(windows)]
    {
        let path_l = path_s.to_ascii_lowercase();
        let root_l = root_s.to_ascii_lowercase();
        if path_l == root_l {
            return Ok(PathBuf::new());
        }
        let prefix_l = format!("{root_l}/");
        if let Some(rest) = path_l.strip_prefix(&prefix_l) {
            // Preserve the caller's casing from the original path suffix.
            return Ok(PathBuf::from(&path_s[path_s.len() - rest.len()..]));
        }
    }

    if let (Ok(c_path), Ok(c_root)) = (path.canonicalize(), worktree.canonicalize()) {
        if let Ok(rel) = c_path.strip_prefix(&c_root) {
            return Ok(rel.to_path_buf());
        }
    }

    Err(DesktopError::Message(format!(
        "file is outside the session workspace (`{}`)",
        worktree.display()
    )))
}

/// Accept a repo-relative path *or* an absolute path under `worktree`
/// (Write/Edit tool inputs are always absolute). Returns a forward-slashed
/// relative path for `git … -- <path>`. Isolation is irrelevant — non-isolated
/// sessions still have absolute tool paths that must strip against `cwd`.
pub(crate) fn resolve_review_path(path: &str, worktree: &std::path::Path) -> DesktopResult<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("path is required".into()));
    }
    let as_path = std::path::Path::new(trimmed);
    let relative = if as_path.is_absolute() {
        strip_worktree_prefix(as_path, worktree)?
    } else {
        validate_repo_relative_path(trimmed)?;
        PathBuf::from(trimmed)
    };
    if relative.as_os_str().is_empty() {
        return Err(DesktopError::Message(
            "path must be a file inside the session workspace, not the workspace root".into(),
        ));
    }
    if relative
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(DesktopError::Message(format!(
            "path must not contain '..': {trimmed}"
        )));
    }
    Ok(normalize_path_slashes(&relative.to_string_lossy()))
}

/// Two-letter `git status --porcelain` code for a single path (e.g. `"??"`,
/// `" M"`, `"D "`), or `None` if the path has no pending changes.
pub(crate) fn porcelain_code(dir: &std::path::Path, path: &str) -> DesktopResult<Option<String>> {
    let out = crate::win_console::command("git")
        .args(["-C"])
        .arg(dir)
        .args(["status", "--porcelain", "--", path])
        .output()
        .map_err(|e| DesktopError::Message(format!("git status failed for `{path}`: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(DesktopError::Message(format!(
            "git status failed for `{path}`: {}",
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let Some(line) = stdout.lines().next() else {
        return Ok(None);
    };
    if line.len() < 2 {
        return Ok(None);
    }
    Ok(Some(line[..2].to_string()))
}

/// `git -C <base_dir> rev-parse HEAD`, used as the stable "pre-agent" base
/// state to diff/restore against for isolated sessions (the worktree's own
/// HEAD can move — `integrate_session` commits agent changes into it).
pub(crate) fn base_head_sha(base_dir: &std::path::Path) -> DesktopResult<String> {
    let out = crate::win_console::command("git")
        .args(["-C"])
        .arg(base_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| {
            DesktopError::Message(format!(
                "git rev-parse HEAD failed in base repo `{}`: {e}",
                base_dir.display()
            ))
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(DesktopError::Message(format!(
            "git rev-parse HEAD failed in base repo `{}`: {}",
            base_dir.display(),
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Resolve the session's working directory + optional base-repo directory,
/// mirroring the `meta.cwd` / `meta.base_cwd` split documented on
/// `SessionMeta`: `cwd` is the worktree root when isolated, else the repo
/// itself; `base_cwd` is `Some` only when isolated.
pub(crate) async fn review_dirs(
    state: &AppState,
    session_id: &str,
) -> DesktopResult<(PathBuf, Option<PathBuf>)> {
    let service = require_service(state).await?;
    let id = SessionId::from(session_id.to_string());
    let meta = service.session_meta(&id).await?;
    Ok((meta.cwd, meta.base_cwd))
}

#[cfg(test)]
mod resolve_review_path_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn keeps_repo_relative_paths() {
        let root = PathBuf::from("/repo");
        assert_eq!(
            resolve_review_path("packages/desktop/src/App.tsx", &root).unwrap(),
            "packages/desktop/src/App.tsx"
        );
    }

    #[test]
    fn strips_absolute_path_under_worktree() {
        let root = PathBuf::from("/repo");
        assert_eq!(
            resolve_review_path("/repo/packages/desktop/src/App.tsx", &root).unwrap(),
            "packages/desktop/src/App.tsx"
        );
    }

    #[test]
    fn rejects_absolute_path_outside_worktree() {
        let root = PathBuf::from("/repo");
        let err = resolve_review_path("/other/file.rs", &root).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("outside the session workspace"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn rejects_parent_dir_segments() {
        let root = PathBuf::from("/repo");
        assert!(resolve_review_path("../secret", &root).is_err());
    }
}
