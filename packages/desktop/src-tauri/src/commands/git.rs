//! Git status, commit, push, PR, and session baselines.

use super::common::{require_service, review_dirs, validate_repo_relative_path};
use super::prelude::*;

#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn git_is_repo(cwd: String) -> bool {
    crate::win_console::command("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(cwd)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Whether `cwd`'s repo has at least one configured remote (`git remote`
/// prints a non-empty name). Gates Commit vs Commit & Push in the UI — no
/// remote means push would fail with "No configured push destination", so
/// the chrome must not offer push.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn git_has_remote(cwd: String) -> bool {
    crate::win_console::command("git")
        .args(["remote"])
        .current_dir(cwd)
        .output()
        .map(|out| out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
        .unwrap_or(false)
}

/// Read-only current-branch lookup for the composer context bar.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn git_branch(cwd: String) -> Option<String> {
    let output = crate::win_console::command("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

/// Local branch names for the branch picker (`git branch --format`).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_list_branches(cwd: String) -> DesktopResult<Vec<String>> {
    let output = crate::win_console::command("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git list branches failed: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git list branches failed".into()
        } else {
            stderr
        }));
    }
    let mut branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    branches.sort();
    branches.dedup();
    Ok(branches)
}

/// Check out a local branch in the session cwd.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_checkout(cwd: String, branch: String) -> DesktopResult<()> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') {
        return Err(DesktopError::Message("invalid branch name".into()));
    }
    let output = crate::win_console::command("git")
        .args(["checkout", branch])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git checkout failed: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            format!("git checkout {branch} failed")
        } else {
            stderr
        }));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    /// Path relative to `cwd` (rename keeps the new path).
    pub path: String,
    /// Porcelain letter: "M" | "A" | "D" | "R" | "?" (untracked) | other.
    pub status: String,
    /// Lines added per `git diff --numstat HEAD`; None for binary/untracked.
    pub added: Option<u32>,
    /// Lines removed; None for binary/untracked.
    pub removed: Option<u32>,
}

/// Max rows returned to the UI by [`git_status`] / [`git_status_since_baseline`].
/// A session that scaffolds a large project (e.g. `create-next-app`) can dirty
/// hundreds of untracked files; rendering all of them as list rows is what
/// makes the Changes panel jank. The UI shows a "+N more" indicator instead of
/// mounting every row, and [`GitStatusSummary`]'s totals are always computed
/// over the *full* set so the aggregate +/- badge stays correct regardless of
/// the cap.
const MAX_STATUS_FILES: usize = 300;

/// Wraps a (possibly truncated) file list with totals computed over the full,
/// untruncated set — the aggregate +/- badge and file count must reflect
/// every changed file even when only the first [`MAX_STATUS_FILES`] rows are
/// sent to the UI for rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusSummary {
    /// First `MAX_STATUS_FILES` entries only — render these as rows.
    pub files: Vec<GitFileStatus>,
    /// Total number of changed files (tracked + untracked), untruncated.
    pub total_count: usize,
    /// Sum of `added` across every changed file, untruncated.
    pub total_added: u32,
    /// Sum of `removed` across every changed file, untruncated.
    pub total_removed: u32,
    /// `true` when `files` was truncated (`total_count > files.len()`).
    pub truncated: bool,
}

pub(crate) fn summarize(mut files: Vec<GitFileStatus>) -> GitStatusSummary {
    let total_count = files.len();
    let mut total_added = 0u32;
    let mut total_removed = 0u32;
    for f in &files {
        total_added += f.added.unwrap_or(0);
        total_removed += f.removed.unwrap_or(0);
    }
    let truncated = total_count > MAX_STATUS_FILES;
    files.truncate(MAX_STATUS_FILES);
    GitStatusSummary {
        files,
        total_count,
        total_added,
        total_removed,
        truncated,
    }
}

/// Read-only working-tree status for the Changes panel. Non-git dirs yield
/// an empty summary (mirrors `git_branch`'s tolerance). Capped at
/// [`MAX_STATUS_FILES`] rows; see [`GitStatusSummary`] for how totals stay
/// accurate past the cap.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_status(cwd: String) -> DesktopResult<GitStatusSummary> {
    tokio::task::spawn_blocking(move || Ok(summarize(git_status_full(&cwd)?)))
        .await
        .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?
}

/// Shared implementation behind [`git_status`] and
/// [`git_status_since_baseline`]. Returns the full, untruncated list —
/// callers cap/summarize via [`summarize`].
pub(crate) fn git_status_full(cwd: &str) -> DesktopResult<Vec<GitFileStatus>> {
    let porcelain = match crate::win_console::command("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => return Ok(Vec::new()),
    };

    // Line counts per changed file; binary files report "-" and are skipped.
    let mut counts: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();
    if let Ok(out) = crate::win_console::command("git")
        .args(["diff", "--numstat", "HEAD"])
        .current_dir(cwd)
        .output()
    {
        if out.status.success() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let mut parts = line.split('\t');
                let (Some(a), Some(r), Some(path)) = (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                if let (Ok(a), Ok(r)) = (a.parse::<u32>(), r.parse::<u32>()) {
                    // Renames appear as "old => new" or "{old => new}/tail".
                    let path = path
                        .rsplit(" => ")
                        .next()
                        .unwrap_or(path)
                        .trim_end_matches('}')
                        .to_string();
                    counts.insert(path, (a, r));
                }
            }
        }
    }

    let mut files = Vec::new();
    for line in porcelain.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let mut path = line[3..].trim().to_string();
        // Rename lines: "R  old -> new" — keep the new path.
        if let Some((_, new)) = path.split_once(" -> ") {
            path = new.trim().to_string();
        }
        // Strip porcelain quoting for paths with special characters.
        if path.starts_with('"') && path.ends_with('"') && path.len() >= 2 {
            path = path[1..path.len() - 1].to_string();
        }
        let status = if code == "??" {
            "?".to_string()
        } else {
            code.trim()
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "M".to_string())
        };
        let (added, removed) = counts
            .get(&path)
            .map(|&(a, r)| (Some(a), Some(r)))
            .unwrap_or((None, None));
        files.push(GitFileStatus {
            path,
            status,
            added,
            removed,
        });
    }
    Ok(files)
}

/// Working-tree status scoped to what this session has actually touched,
/// for the Changes panel. Falls back to the full-repo [`git_status`] result
/// (unchanged shape) whenever session-scoping isn't possible or safe:
///
/// - Isolated sessions (`base_cwd.is_some()`) already run in a private
///   worktree, so the plain status is already session-scoped.
/// - No baseline was captured for this session (e.g. it predates this
///   feature, or the app restarted between creation and baseline capture
///   and `resume_session` hasn't run yet).
/// - The repo's HEAD has moved since the baseline was captured (a commit,
///   checkout, etc. invalidates the recorded content hashes' meaning).
///
/// Otherwise, a dirty path from the current `git status` is kept only if it
/// wasn't dirty at baseline time, or its content hash has changed since.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_status_since_baseline(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<GitStatusSummary> {
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    let cwd_str = cwd.to_string_lossy().to_string();
    let cwd_path = cwd.clone();

    if base_cwd.is_some() {
        let cwd_for_git = cwd_str.clone();
        return tokio::task::spawn_blocking(move || Ok(summarize(git_status_full(&cwd_for_git)?)))
            .await
            .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?;
    }

    let baseline = {
        let baselines = state.session_baselines.lock().await;
        baselines
            .get(&session_id)
            .map(|b| (b.head_sha.clone(), b.files.clone()))
    };
    let Some((baseline_head, baseline_files)) = baseline else {
        let cwd_for_git = cwd_str.clone();
        return tokio::task::spawn_blocking(move || Ok(summarize(git_status_full(&cwd_for_git)?)))
            .await
            .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?;
    };

    tokio::task::spawn_blocking(move || {
        if current_head_sha(&cwd_path) != baseline_head {
            return Ok(summarize(git_status_full(&cwd_str)?));
        }

        let all = git_status_full(&cwd_str)?;
        let paths_to_hash: Vec<String> = all
            .iter()
            .filter(|f| {
                matches!(
                    baseline_files.get(&f.path),
                    Some(h) if h.as_str() != "dir"
                )
            })
            .map(|f| f.path.clone())
            .collect();
        let hashes = hash_objects_batch(&cwd_path, &paths_to_hash);
        let filtered = all
            .into_iter()
            .filter(|f| match baseline_files.get(&f.path) {
                None => true,
                // Untracked dir already recorded at baseline time (see the "dir"
                // sentinel in `capture_session_baseline`) — there's no blob to
                // hash for a directory, and an already-untracked dir isn't a
                // session change, so it's always filtered out regardless of what
                // may have changed inside it (mirrors git's own porcelain
                // granularity, which also collapses to the single dir entry).
                Some(baseline_hash) if baseline_hash == "dir" => false,
                Some(baseline_hash) => {
                    let current_hash = hashes
                        .get(&f.path)
                        .cloned()
                        .unwrap_or_else(|| "deleted".to_string());
                    &current_hash != baseline_hash
                }
            })
            .collect();
        Ok(summarize(filtered))
    })
    .await
    .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?
}

/// `git hash-object <path>` relative to `cwd`; used to detect whether a
/// dirty path's content has changed since baseline capture. Returns `None`
/// on any git failure (missing file, not a git repo, etc.) so callers can
/// treat the path as "unknown" rather than failing outright.
pub(crate) fn hash_object(cwd: &std::path::Path, path: &str) -> Option<String> {
    let out = crate::win_console::command("git")
        .args(["hash-object", path])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Batch `git hash-object --stdin-paths` for many relative paths in one
/// subprocess. Falls back to per-path [`hash_object`] if the batch fails
/// (e.g. a path vanished mid-flight). Missing paths are simply omitted from
/// the map so callers can treat them as deleted.
pub(crate) fn hash_objects_batch(
    cwd: &std::path::Path,
    paths: &[String],
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    if paths.is_empty() {
        return out;
    }

    // Only feed paths git can hash — `--stdin-paths` aborts the whole batch
    // if any path is missing. Include symlinks (even to dirs): `is_file()`
    // follows links and would skip them, wrongly baselining as "deleted".
    // Real directories are excluded; porcelain already records those as "dir".
    let existing: Vec<&str> = paths
        .iter()
        .map(String::as_str)
        .filter(|p| {
            let meta = match cwd.join(p).symlink_metadata() {
                Ok(m) => m,
                Err(_) => return false,
            };
            meta.is_file() || meta.file_type().is_symlink()
        })
        .collect();
    if existing.is_empty() {
        return out;
    }

    if let Some(batch) = try_hash_objects_stdin(cwd, &existing) {
        return batch;
    }

    for path in existing {
        if let Some(hash) = hash_object(cwd, path) {
            out.insert(path.to_string(), hash);
        }
    }
    out
}

pub(crate) fn try_hash_objects_stdin(
    cwd: &std::path::Path,
    paths: &[&str],
) -> Option<std::collections::HashMap<String, String>> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = crate::win_console::command("git")
        .args(["hash-object", "--stdin-paths"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    {
        let mut stdin = child.stdin.take()?;
        for path in paths {
            writeln!(stdin, "{path}").ok()?;
        }
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let hashes: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if hashes.len() != paths.len() {
        return None;
    }
    Some(
        paths
            .iter()
            .zip(hashes)
            .map(|(p, h)| ((*p).to_string(), h))
            .collect(),
    )
}

/// `git rev-parse HEAD` in `cwd`; empty string if there is no HEAD yet
/// (e.g. a freshly initialized repo with no commits) rather than an error,
/// since that's a legitimate baseline state.
pub(crate) fn current_head_sha(cwd: &std::path::Path) -> String {
    crate::win_console::command("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Capture a [`crate::state::SessionBaseline`] snapshot of `cwd`'s dirty
/// state, for scoping the Changes panel to this session's own edits. Only
/// meaningful for non-isolated sessions (isolated sessions already get a
/// clean worktree, so their `git_status` is inherently session-scoped).
///
/// Non-fatal by design: any git failure (not a repo, git missing, etc.)
/// simply yields no baseline, and `git_status_since_baseline` gracefully
/// degrades to the full-repo `git_status` in that case.
pub(crate) fn capture_session_baseline(
    cwd: &std::path::Path,
) -> Option<crate::state::SessionBaseline> {
    let porcelain = crate::win_console::command("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !porcelain.status.success() {
        return None;
    }
    let head_sha = current_head_sha(cwd);

    let mut pending_hash: Vec<String> = Vec::new();
    let mut files = std::collections::HashMap::new();
    for line in String::from_utf8_lossy(&porcelain.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[..2];
        let mut path = line[3..].trim().to_string();
        if let Some((_, new)) = path.split_once(" -> ") {
            path = new.trim().to_string();
        }
        if path.starts_with('"') && path.ends_with('"') && path.len() >= 2 {
            path = path[1..path.len() - 1].to_string();
        }
        // Untracked dirs are reported by porcelain as a single "dir/" entry
        // with no single blob to hash. Record them with the "dir" sentinel
        // instead of skipping them outright: skipping meant an
        // already-untracked dir at baseline time was absent from
        // `baseline.files`, so `git_status_since_baseline`'s filter (`None`
        // => "not in baseline" => keep) treated it as a brand-new session
        // change — the "phantom session changes" bug. With the sentinel
        // recorded, that same dir entry is now `Some("dir")` in the filter
        // and gets correctly dropped as pre-existing. A dir newly created
        // during the session still has no baseline entry at all, so it's
        // still kept. Note: files added inside an already-untracked dir stay
        // collapsed under the single dir entry by porcelain itself (git's
        // own display has the same granularity) — acceptable.
        if path.ends_with('/') {
            files.insert(path, "dir".to_string());
            continue;
        }
        let is_deleted = code.contains('D');
        if is_deleted {
            files.insert(path, "deleted".to_string());
        } else {
            pending_hash.push(path);
        }
    }

    let hashes = hash_objects_batch(cwd, &pending_hash);
    for path in pending_hash {
        let hash = hashes
            .get(&path)
            .cloned()
            .unwrap_or_else(|| "deleted".to_string());
        files.insert(path, hash);
    }

    Some(crate::state::SessionBaseline { head_sha, files })
}

const MAX_DIFF_BYTES: usize = 200 * 1024;

/// Truncate `text` to `MAX_DIFF_BYTES` at a char boundary, appending a marker
/// so callers can tell the diff was cut short. Shared by all diff commands.
pub(crate) fn truncate_diff(mut text: String) -> String {
    if text.len() > MAX_DIFF_BYTES {
        let mut cut = MAX_DIFF_BYTES;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n… diff truncated …\n");
    }
    text
}

/// `git diff <rev> -- <path>` in `dir`, falling back to a `--no-index` diff
/// against `/dev/null` when the file has no history against `rev` (i.e. it's
/// untracked there). Shared by `git_diff` and `review_file_diff`.
pub(crate) fn diff_against_rev(
    dir: &std::path::Path,
    rev: &str,
    path: &str,
) -> DesktopResult<String> {
    let tracked = crate::win_console::command("git")
        .args(["diff", rev, "--", path])
        .current_dir(dir)
        .output()
        .map_err(|e| DesktopError::Message(format!("git diff failed: {e}")))?;

    let mut text = if tracked.status.success() {
        String::from_utf8_lossy(&tracked.stdout).to_string()
    } else {
        String::new()
    };

    if text.trim().is_empty() {
        // Untracked file: diff against /dev/null (exit code 1 means "differs",
        // which is success for --no-index; >1 is a real error).
        let untracked = crate::win_console::command("git")
            .args(["diff", "--no-index", "--", "/dev/null", path])
            .current_dir(dir)
            .output()
            .map_err(|e| DesktopError::Message(format!("git diff failed: {e}")))?;
        match untracked.status.code() {
            Some(0) | Some(1) => {
                text = String::from_utf8_lossy(&untracked.stdout).to_string();
            }
            _ => {
                let stderr = String::from_utf8_lossy(&untracked.stderr)
                    .trim()
                    .to_string();
                return Err(DesktopError::Message(if stderr.is_empty() {
                    "git diff failed".into()
                } else {
                    stderr
                }));
            }
        }
    }

    Ok(truncate_diff(text))
}

/// Unified diff for one file (read-only, capped) for the Changes panel.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_diff(cwd: String, path: String) -> DesktopResult<String> {
    let path = path.trim();
    if path.is_empty() || path.starts_with('-') {
        return Err(DesktopError::Message("invalid path".into()));
    }
    diff_against_rev(std::path::Path::new(&cwd), "HEAD", path)
}

/// Stage everything and commit in the session's working directory, for the
/// "Commit & Push" bar above the composer. Isolated sessions
/// integrate their worktree back into the base repo instead (`integrate_session`)
/// — committing directly here would strand the commit in a throwaway worktree —
/// so this is rejected up front for those sessions.
///
/// Returns the resulting commit's short SHA.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_commit(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
) -> DesktopResult<String> {
    let message = message.trim();
    if message.is_empty() {
        return Err(DesktopError::Message("commit message is required".into()));
    }
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let add = crate::win_console::command("git")
        .args(["add", "-A"])
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git add failed: {e}")))?;
    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git add failed".into()
        } else {
            stderr
        }));
    }

    let commit = crate::win_console::command("git")
        .args(["commit", "-m"])
        .arg(message)
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git commit failed: {e}")))?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git commit failed".into()
        } else {
            stderr
        }));
    }

    let sha = crate::win_console::command("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git rev-parse failed: {e}")))?;
    if !sha.status.success() {
        let stderr = String::from_utf8_lossy(&sha.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git rev-parse failed".into()
        } else {
            stderr
        }));
    }
    Ok(String::from_utf8_lossy(&sha.stdout).trim().to_string())
}

/// Push the current branch in the session's working directory. Same
/// isolated-session restriction as `git_commit` (see its doc comment).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_push(state: State<'_, AppState>, session_id: String) -> DesktopResult<()> {
    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let push = crate::win_console::command("git")
        .args(["push"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git push failed: {e}")))?;
    if !push.status.success() {
        let stderr = String::from_utf8_lossy(&push.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git push failed".into()
        } else {
            stderr
        }));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Commit center: selective staging + commit/push/branch/PR flow for the
// Changes tab (spec #48). Same isolated-session restriction as
// `git_commit`/`git_push` above — isolated sessions integrate their worktree
// back into the base repo instead of committing directly here.
// ---------------------------------------------------------------------------

/// Push the current branch, creating the upstream on first push (`git push
/// -u origin <branch>`) instead of failing with "no upstream branch". Shared
/// by `git_commit_and_push` and `git_create_pr`.
pub(crate) fn push_current_branch(cwd: &std::path::Path) -> DesktopResult<()> {
    let push = crate::win_console::command("git")
        .args(["push"])
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git push failed: {e}")))?;
    if push.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&push.stderr).trim().to_string();
    // "no upstream" is reported on stderr by git; retry with `-u origin
    // <branch>` rather than string-matching the exact wording, which varies
    // by git version/locale — instead just always retry once with `-u` on
    // any push failure that looks like a missing-upstream case.
    if stderr.contains("has no upstream branch") || stderr.contains("--set-upstream") {
        let branch = git_branch(cwd.to_string_lossy().to_string())
            .ok_or_else(|| DesktopError::Message("could not determine current branch".into()))?;
        let retry = crate::win_console::command("git")
            .args(["push", "-u", "origin", &branch])
            .current_dir(cwd)
            .output()
            .map_err(|e| DesktopError::Message(format!("git push -u failed: {e}")))?;
        if retry.status.success() {
            return Ok(());
        }
        let retry_stderr = String::from_utf8_lossy(&retry.stderr).trim().to_string();
        return Err(DesktopError::Message(if retry_stderr.is_empty() {
            "git push -u failed".into()
        } else {
            retry_stderr
        }));
    }
    Err(DesktopError::Message(if stderr.is_empty() {
        "git push failed".into()
    } else {
        stderr
    }))
}

/// Stage only `paths` (`git add -- <paths>`) then commit. Shared staging +
/// commit body for every commit-center entry point below. Rejects isolated
/// sessions and empty message/paths up front — same contract as `git_commit`.
pub(crate) async fn commit_selected_paths(
    state: &State<'_, AppState>,
    session_id: &str,
    message: &str,
    paths: &[String],
) -> DesktopResult<(PathBuf, String)> {
    let message = message.trim();
    if message.is_empty() {
        return Err(DesktopError::Message("commit message is required".into()));
    }
    if paths.is_empty() {
        return Err(DesktopError::Message(
            "select at least one file to commit".into(),
        ));
    }
    let mut relative_paths = Vec::with_capacity(paths.len());
    for p in paths {
        relative_paths.push(validate_repo_relative_path(p)?.to_string());
    }

    let (cwd, base_cwd) = review_dirs(state, session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let mut add_cmd = crate::win_console::command("git");
    add_cmd.arg("add").arg("--").args(&relative_paths);
    let add = add_cmd
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git add failed: {e}")))?;
    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git add failed".into()
        } else {
            stderr
        }));
    }

    let commit = crate::win_console::command("git")
        .args(["commit", "-m"])
        .arg(message)
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git commit failed: {e}")))?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git commit failed".into()
        } else {
            stderr
        }));
    }

    let sha = crate::win_console::command("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git rev-parse failed: {e}")))?;
    if !sha.status.success() {
        let stderr = String::from_utf8_lossy(&sha.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            "git rev-parse failed".into()
        } else {
            stderr
        }));
    }
    Ok((cwd, String::from_utf8_lossy(&sha.stdout).trim().to_string()))
}

/// Stage exactly the selected files and commit — the Changes tab's per-file
/// checkbox selection, unlike `git_commit`'s `git add -A`.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_commit_paths(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    paths: Vec<String>,
) -> DesktopResult<String> {
    let (_cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    Ok(sha)
}

/// Commit the selected files, then push (creating the upstream if this is
/// the branch's first push).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_commit_and_push(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    paths: Vec<String>,
) -> DesktopResult<String> {
    let (cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    push_current_branch(&cwd)?;
    Ok(sha)
}

/// Create and check out a new local branch, then commit the selected files
/// to it. The branch is created off the current HEAD (`git checkout -b`).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_create_branch_and_commit(
    state: State<'_, AppState>,
    session_id: String,
    branch: String,
    message: String,
    paths: Vec<String>,
) -> DesktopResult<String> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') {
        return Err(DesktopError::Message("invalid branch name".into()));
    }

    let (cwd, base_cwd) = review_dirs(&state, &session_id).await?;
    if base_cwd.is_some() {
        return Err(DesktopError::Message(
            "isolated sessions integrate instead".into(),
        ));
    }

    let checkout = crate::win_console::command("git")
        .args(["checkout", "-b", branch])
        .current_dir(&cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("git checkout -b failed: {e}")))?;
    if !checkout.status.success() {
        let stderr = String::from_utf8_lossy(&checkout.stderr).trim().to_string();
        return Err(DesktopError::Message(if stderr.is_empty() {
            format!("git checkout -b {branch} failed")
        } else {
            stderr
        }));
    }

    let (_cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    Ok(sha)
}

/// Commit the selected files, push the branch, then open a PR via `gh pr
/// create --fill` (or with an explicit title/body when given). Gracefully
/// degrades when the GitHub CLI isn't installed or isn't authenticated: the
/// branch is still pushed (so the commit is never stranded locally-only) and
/// the returned message tells the UI to show "GitHub CLI not available —
/// pushed the branch instead" rather than silently losing the PR step.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_create_pr(
    state: State<'_, AppState>,
    session_id: String,
    message: String,
    paths: Vec<String>,
    title: Option<String>,
    body: Option<String>,
) -> DesktopResult<CreatePrOutcome> {
    let (cwd, sha) = commit_selected_paths(&state, &session_id, &message, &paths).await?;
    push_current_branch(&cwd)?;

    let available = gh_available(&cwd);
    if !available {
        return Ok(CreatePrOutcome {
            commit_sha: sha,
            pr_url: None,
            degraded_reason: Some("GitHub CLI not available — pushed the branch instead".into()),
        });
    }

    let pr = run_gh_pr_create(&cwd, title.as_deref(), body.as_deref())?;
    if !pr.status.success() {
        let stderr = String::from_utf8_lossy(&pr.stderr).trim().to_string();
        // The push already succeeded above, so degrade rather than error —
        // the commit/push is not lost even though the PR step failed (e.g.
        // a PR already exists for this branch, or `gh` isn't authenticated
        // for this repo's host).
        return Ok(CreatePrOutcome {
            commit_sha: sha,
            pr_url: None,
            degraded_reason: Some(if stderr.is_empty() {
                "gh pr create failed — pushed the branch instead".into()
            } else {
                format!("gh pr create failed — pushed the branch instead ({stderr})")
            }),
        });
    }
    let url = String::from_utf8_lossy(&pr.stdout).trim().to_string();
    Ok(CreatePrOutcome {
        commit_sha: sha,
        pr_url: (!url.is_empty()).then_some(url),
        degraded_reason: None,
    })
}

/// Build `gh pr create` with either an explicit title/body or `--fill`.
pub(crate) fn run_gh_pr_create(
    cwd: &std::path::Path,
    title: Option<&str>,
    body: Option<&str>,
) -> DesktopResult<std::process::Output> {
    let mut pr_cmd = crate::win_console::command("gh");
    pr_cmd.arg("pr").arg("create");
    match (title, body) {
        (Some(t), Some(b)) if !t.trim().is_empty() => {
            pr_cmd.arg("--title").arg(t).arg("--body").arg(b);
        }
        (Some(t), None) if !t.trim().is_empty() => {
            pr_cmd.arg("--title").arg(t).arg("--body").arg("");
        }
        _ => {
            pr_cmd.arg("--fill");
        }
    }
    pr_cmd
        .current_dir(cwd)
        .output()
        .map_err(|e| DesktopError::Message(format!("gh pr create failed: {e}")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrOutcome {
    pub commit_sha: String,
    pub pr_url: Option<String>,
    /// Set when the PR step itself was skipped/failed but the commit+push
    /// still succeeded (e.g. `gh` missing/unauthenticated) — the UI shows
    /// this as a non-fatal toast rather than treating the call as an error.
    pub degraded_reason: Option<String>,
}

/// Whether `gh` is installed and authenticated for this cwd.
pub(crate) fn gh_available(cwd: &std::path::Path) -> bool {
    let gh_check = crate::win_console::command("gh")
        .args(["auth", "status"])
        .current_dir(cwd)
        .output();
    matches!(&gh_check, Ok(out) if out.status.success())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchPrInfo {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// OPEN / MERGED / CLOSED (from `gh pr view --json state`).
    pub state: String,
    /// Human summary derived from `statusCheckRollup`, e.g. "3/3 passing".
    pub checks_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchPrStatus {
    pub gh_available: bool,
    pub pr: Option<BranchPrInfo>,
}

/// Summarize `gh pr view --json statusCheckRollup` into a short chip label.
pub(crate) fn summarize_status_checks(rollup: &[serde_json::Value]) -> String {
    if rollup.is_empty() {
        return "No checks".into();
    }
    let mut passing = 0u32;
    let mut failing = 0u32;
    let mut pending = 0u32;
    for item in rollup {
        let conclusion = item
            .get("conclusion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        let status = item
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        // StatusContext nodes use `state` instead of conclusion/status.
        let state = item
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_uppercase();

        if matches!(conclusion.as_str(), "SUCCESS" | "NEUTRAL" | "SKIPPED") || state == "SUCCESS" {
            passing += 1;
        } else if matches!(
            conclusion.as_str(),
            "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED"
        ) || matches!(state.as_str(), "FAILURE" | "ERROR")
        {
            failing += 1;
        } else if status == "COMPLETED" && conclusion.is_empty() && state.is_empty() {
            passing += 1;
        } else {
            pending += 1;
        }
    }
    let total = passing + failing + pending;
    if failing > 0 {
        format!("{failing}/{total} failing")
    } else if pending > 0 {
        format!("{pending}/{total} pending")
    } else {
        format!("{passing}/{total} passing")
    }
}

/// Look up the open PR for the current branch via `gh pr view`. Returns
/// `pr: None` when there is no PR (or `gh` is unavailable) — never an error
/// for the common "no PR yet" case, so the Changes UI can poll safely.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_pr_status(cwd: String) -> DesktopResult<BranchPrStatus> {
    let path = std::path::PathBuf::from(&cwd);
    if !gh_available(&path) {
        return Ok(BranchPrStatus {
            gh_available: false,
            pr: None,
        });
    }

    let out = crate::win_console::command("gh")
        .args([
            "pr",
            "view",
            "--json",
            "number,title,url,state,statusCheckRollup",
        ])
        .current_dir(&path)
        .output()
        .map_err(|e| DesktopError::Message(format!("gh pr view failed: {e}")))?;

    if !out.status.success() {
        // No PR for this branch (or other non-fatal gh exits) — treat as empty.
        return Ok(BranchPrStatus {
            gh_available: true,
            pr: None,
        });
    }

    let raw = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(raw.trim())
        .map_err(|e| DesktopError::Message(format!("gh pr view returned invalid JSON: {e}")))?;

    let number = value
        .get("number")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| DesktopError::Message("gh pr view missing number".into()))?;
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let url = value
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let state = value
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("OPEN")
        .to_string();
    let rollup = value
        .get("statusCheckRollup")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[]);

    Ok(BranchPrStatus {
        gh_available: true,
        pr: Some(BranchPrInfo {
            number,
            title,
            url,
            state,
            checks_summary: summarize_status_checks(rollup),
        }),
    })
}

/// Unified diff for the current branch's PR via `gh pr diff`. Empty string
/// when there is no PR / `gh` unavailable — callers poll alongside
/// `git_pr_status` and should treat empty as "nothing to review".
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_pr_diff(cwd: String) -> DesktopResult<String> {
    let path = std::path::PathBuf::from(&cwd);
    if !gh_available(&path) {
        return Ok(String::new());
    }

    let out = crate::win_console::command("gh")
        .args(["pr", "diff"])
        .current_dir(&path)
        .output()
        .map_err(|e| DesktopError::Message(format!("gh pr diff failed: {e}")))?;

    if !out.status.success() {
        return Ok(String::new());
    }

    let mut diff = String::from_utf8_lossy(&out.stdout).into_owned();
    // Cap so a huge monorepo PR cannot freeze the DiffView.
    const MAX_CHARS: usize = 512_000;
    if diff.len() > MAX_CHARS {
        diff.truncate(MAX_CHARS);
        diff.push_str("\n\n… diff truncated …\n");
    }
    Ok(diff)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrDraft {
    pub title: String,
    pub body: String,
}

/// Prefill title/body for the Create PR dialog — latest commit subject as
/// title, and bullet subjects for any additional commits ahead of the
/// upstream (or the repo's default branch when no upstream is set). Empty
/// strings when git can't resolve a suggestion; the UI still opens.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_pr_draft(cwd: String) -> DesktopResult<PrDraft> {
    let path = std::path::PathBuf::from(&cwd);
    let title = crate::win_console::command("git")
        .args(["log", "-1", "--pretty=%s"])
        .current_dir(&path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Prefer commits not yet on the upstream tracking branch; fall back to
    // the remote HEAD / origin/main so a freshly pushed feature branch still
    // gets a useful multi-commit body.
    let range_candidates = [
        "@{upstream}..HEAD",
        "origin/HEAD..HEAD",
        "origin/main..HEAD",
        "origin/master..HEAD",
    ];
    let mut body = String::new();
    for range in range_candidates {
        let out = crate::win_console::command("git")
            .args(["log", range, "--pretty=format:- %s"])
            .current_dir(&path)
            .output();
        let Ok(out) = out else { continue };
        if !out.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if text.is_empty() {
            continue;
        }
        // Skip a single-bullet body that just repeats the title — leave the
        // description empty so the dialog doesn't look redundant.
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() == 1 {
            let subject = lines[0].trim_start_matches("- ").trim();
            if subject == title.trim() {
                body.clear();
            } else {
                body = text;
            }
        } else {
            body = text;
        }
        break;
    }

    Ok(PrDraft { title, body })
}

/// Create a PR for the current branch without a fresh commit (branch must
/// already have commits to open against the base). Optional `title`/`body`
/// override `gh`'s `--fill`; omit both (or pass empty title) to fill from
/// commits. Pushes first when a remote is configured so the head ref exists
/// on the host. Same degradation as `git_create_pr` when `gh` is unavailable.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_create_pr_for_branch(
    cwd: String,
    title: Option<String>,
    body: Option<String>,
) -> DesktopResult<CreatePrOutcome> {
    let path = std::path::PathBuf::from(&cwd);
    // Best-effort push so a local-only branch can still become a PR head.
    let _ = push_current_branch(&path);

    if !gh_available(&path) {
        return Ok(CreatePrOutcome {
            commit_sha: String::new(),
            pr_url: None,
            degraded_reason: Some(
                "GitHub CLI not available — push the branch and create a PR from the host".into(),
            ),
        });
    }

    let pr = run_gh_pr_create(&path, title.as_deref(), body.as_deref())?;
    if !pr.status.success() {
        let stderr = String::from_utf8_lossy(&pr.stderr).trim().to_string();
        // If a PR already exists, surface its URL instead of failing.
        if let Ok(view) = crate::win_console::command("gh")
            .args(["pr", "view", "--json", "url"])
            .current_dir(&path)
            .output()
        {
            if view.status.success() {
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&view.stdout) {
                    if let Some(url) = value.get("url").and_then(|v| v.as_str()) {
                        if !url.is_empty() {
                            return Ok(CreatePrOutcome {
                                commit_sha: String::new(),
                                pr_url: Some(url.to_string()),
                                degraded_reason: None,
                            });
                        }
                    }
                }
            }
        }
        return Ok(CreatePrOutcome {
            commit_sha: String::new(),
            pr_url: None,
            degraded_reason: Some(if stderr.is_empty() {
                "gh pr create failed".into()
            } else {
                format!("gh pr create failed ({stderr})")
            }),
        });
    }
    let url = String::from_utf8_lossy(&pr.stdout).trim().to_string();
    Ok(CreatePrOutcome {
        commit_sha: String::new(),
        pr_url: (!url.is_empty()).then_some(url),
        degraded_reason: None,
    })
}

/// One-shot, tool-free commit-message suggestion from a diff summary —
/// same throwaway-completion pattern as `suggest_session_title` (no tools,
/// no persistence, no transcript). Any failure (no model set, provider
/// error, empty output) surfaces as an `Err`; callers should just leave the
/// message box empty rather than block the commit flow on this.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn suggest_commit_message(
    state: State<'_, AppState>,
    session_id: String,
    diff_summary: String,
) -> DesktopResult<String> {
    let service = require_service(&state).await?;
    let id = SessionId::from(session_id);
    let meta = service.session_meta(&id).await?;
    let model = meta
        .model
        .ok_or_else(|| DesktopError::Message("session has no model set".into()))?;

    let registry = service.provider_registry();
    let (provider, model_id) = registry
        .resolve(&model)
        .ok_or_else(|| DesktopError::Message(format!("no provider for model {model}")))?;

    let truncated: String = diff_summary.chars().take(4000).collect();
    let system = "Write a concise, imperative-mood git commit message (like \"Fix\", \
        \"Add\", \"Update\") summarizing the diff below. One line, under 72 characters, \
        no trailing period, no quotes. Reply with the commit message only — nothing else."
        .to_string();
    let mut request = ChatRequest::new(model_id, vec![Message::user(truncated)]);
    request.system = Some(system);
    request.max_tokens = Some(64);

    let cancel = CancellationToken::new();
    let mut stream = provider
        .stream_chat(request, cancel)
        .await
        .map_err(|e| DesktopError::Message(e.to_string()))?;

    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event.map_err(|e| DesktopError::Message(e.to_string()))? {
            ProviderStreamEvent::MarkdownDelta { text: delta } => {
                text.push_str(&delta);
            }
            ProviderStreamEvent::MessageEnd { .. } => break,
            _ => {}
        }
    }

    let suggestion = text.trim().trim_matches(['"', '\'']).trim().to_string();
    if suggestion.is_empty() {
        return Err(DesktopError::Message(
            "empty commit message generated".into(),
        ));
    }
    Ok(suggestion)
}

#[cfg(test)]
mod git_status_tests {
    use super::*;

    /// `summarize` must cap rendered rows at `MAX_STATUS_FILES` while keeping
    /// totals (count/added/removed) computed over the *full*, untruncated
    /// set — otherwise the aggregate +/- badge would silently undercount once
    /// a session's changes exceed the row cap (e.g. after scaffolding a
    /// project with hundreds of new files).
    #[test]
    fn summarize_caps_rows_but_keeps_full_totals() {
        let n = MAX_STATUS_FILES + 50;
        let files: Vec<GitFileStatus> = (0..n)
            .map(|i| GitFileStatus {
                path: format!("file_{i}.txt"),
                status: "?".to_string(),
                added: Some(1),
                removed: Some(0),
            })
            .collect();

        let summary = summarize(files);
        assert_eq!(summary.files.len(), MAX_STATUS_FILES);
        assert_eq!(summary.total_count, n);
        assert_eq!(summary.total_added, n as u32);
        assert_eq!(summary.total_removed, 0);
        assert!(summary.truncated);
    }

    /// Below the cap, nothing is truncated and totals match the row count.
    #[test]
    fn summarize_untruncated_when_under_cap() {
        let files: Vec<GitFileStatus> = (0..5)
            .map(|i| GitFileStatus {
                path: format!("file_{i}.txt"),
                status: "M".to_string(),
                added: Some(2),
                removed: Some(1),
            })
            .collect();

        let summary = summarize(files);
        assert_eq!(summary.files.len(), 5);
        assert_eq!(summary.total_count, 5);
        assert_eq!(summary.total_added, 10);
        assert_eq!(summary.total_removed, 5);
        assert!(!summary.truncated);
    }

    #[test]
    fn git_has_remote_false_without_remotes() {
        let dir = std::env::temp_dir().join(format!(
            "flex-git-has-remote-none-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let out = crate::win_console::command("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(
            !git_has_remote(dir.to_string_lossy().into_owned()),
            "fresh init must report no remotes"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn git_has_remote_true_with_origin() {
        let dir = std::env::temp_dir().join(format!(
            "flex-git-has-remote-origin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let init = crate::win_console::command("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(init.status.success());
        let add = crate::win_console::command("git")
            .args(["remote", "add", "origin", "https://example.com/repo.git"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(add.status.success());
        assert!(
            git_has_remote(dir.to_string_lossy().into_owned()),
            "configured origin must count as a push remote"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod session_baseline_tests {
    use super::*;

    /// Minimal git repo fixture under a fresh temp dir (not the shared
    /// scratchpad — each test gets its own throwaway repo so runs don't
    /// interfere). Returns the repo root.
    fn init_repo() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "flex-session-baseline-test-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let out = crate::win_console::command("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        dir
    }

    fn write(dir: &std::path::Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn commit_all(dir: &std::path::Path, msg: &str) {
        crate::win_console::command("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
            .args(["commit", "-q", "-m", msg])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// Baseline capture + filtering should hide a file that was already
    /// dirty before the session started (pre-existing repo mess), while
    /// surfacing a file the "session" newly touched.
    #[test]
    fn baseline_filters_pre_existing_dirty_file_but_keeps_new_edit() {
        let dir = init_repo();

        // Committed baseline: two tracked files.
        write(&dir, "pre_dirty.txt", "original\n");
        write(&dir, "untouched.txt", "original\n");
        commit_all(&dir, "initial commit");

        // Simulate a pre-existing dirty file the user had before opening a
        // session in this repo.
        write(&dir, "pre_dirty.txt", "user's uncommitted edit\n");

        // Capture the session baseline now (mirrors create_session).
        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");
        assert!(!baseline.head_sha.is_empty());
        assert!(baseline.files.contains_key("pre_dirty.txt"));

        // Now the "session" makes its own edit to a different file, plus a
        // brand-new untracked file.
        write(&dir, "untouched.txt", "session edit\n");
        write(&dir, "session_new.txt", "brand new\n");

        let cwd_str = dir.to_string_lossy().to_string();
        let all = git_status_full(&cwd_str).expect("git_status_full should succeed");
        let all_paths: Vec<_> = all.iter().map(|f| f.path.as_str()).collect();
        assert!(all_paths.contains(&"pre_dirty.txt"));
        assert!(all_paths.contains(&"untouched.txt"));
        assert!(all_paths.contains(&"session_new.txt"));

        // Reproduce git_status_since_baseline's filtering logic directly
        // (it otherwise requires a Tauri State<AppState>).
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|f| match baseline.files.get(&f.path) {
                None => true,
                Some(baseline_hash) => {
                    let current_hash =
                        hash_object(&dir, &f.path).unwrap_or_else(|| "deleted".to_string());
                    &current_hash != baseline_hash
                }
            })
            .map(|f| f.path)
            .collect();

        assert!(
            !filtered.contains(&"pre_dirty.txt".to_string()),
            "pre-existing dirty file must be filtered out: {filtered:?}"
        );
        assert!(
            filtered.contains(&"untouched.txt".to_string()),
            "session's own edit must survive filtering: {filtered:?}"
        );
        assert!(
            filtered.contains(&"session_new.txt".to_string()),
            "session's new untracked file must survive filtering: {filtered:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A path already dirty at baseline time, but further modified since,
    /// must still show up (content hash differs from the recorded one).
    #[test]
    fn baseline_keeps_file_when_further_modified_after_capture() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");

        write(&dir, "a.txt", "v2 (dirty before session)\n");
        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");

        // Session further edits the same file.
        write(&dir, "a.txt", "v3 (session edit)\n");

        let current_hash = hash_object(&dir, "a.txt").unwrap();
        let baseline_hash = baseline.files.get("a.txt").unwrap();
        assert_ne!(
            &current_hash, baseline_hash,
            "hash must change after the session's own edit"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Deleted-at-baseline paths are recorded with the sentinel so a later
    /// re-creation of the same path is treated as new content, not as
    /// "unchanged since baseline".
    #[test]
    fn baseline_records_deleted_sentinel() {
        let dir = init_repo();
        write(&dir, "gone.txt", "will be deleted\n");
        commit_all(&dir, "initial commit");

        std::fs::remove_file(dir.join("gone.txt")).unwrap();
        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");

        assert_eq!(
            baseline.files.get("gone.txt").map(String::as_str),
            Some("deleted")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Reproduces the "phantom session changes" bug: an untracked directory
    /// that already existed before the session started must not show up as
    /// a session change, while a directory newly created during the session
    /// still must.
    #[test]
    fn baseline_filters_pre_existing_untracked_dir_but_keeps_new_dir() {
        let dir = init_repo();
        write(&dir, "README.md", "hello\n");
        commit_all(&dir, "initial commit");

        // Pre-existing untracked directory (e.g. a build output dir the user
        // already had before opening a session in this repo).
        write(&dir, "public/index.html", "<html></html>\n");

        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");
        assert_eq!(
            baseline.files.get("public/").map(String::as_str),
            Some("dir"),
            "pre-existing untracked dir must be recorded with the dir sentinel: {:?}",
            baseline.files
        );

        // Session creates a brand-new untracked directory of its own.
        write(&dir, "src/new_module.rs", "// new\n");

        let cwd_str = dir.to_string_lossy().to_string();
        let all = git_status_full(&cwd_str).expect("git_status_full should succeed");
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|f| match baseline.files.get(&f.path) {
                None => true,
                Some(baseline_hash) if baseline_hash == "dir" => false,
                Some(baseline_hash) => {
                    let current_hash =
                        hash_object(&dir, &f.path).unwrap_or_else(|| "deleted".to_string());
                    &current_hash != baseline_hash
                }
            })
            .map(|f| f.path)
            .collect();

        assert!(
            !filtered.contains(&"public/".to_string()),
            "pre-existing untracked dir must be filtered out: {filtered:?}"
        );
        assert!(
            filtered.contains(&"src/".to_string()),
            "newly created dir during the session must survive filtering: {filtered:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Persistence round-trip: saving a baseline map to disk and loading it
    /// back must yield an equivalent map. This is the core of the app-restart
    /// fix — `AppState::new` calls `load_session_baselines()` on startup, so
    /// whatever `save_session_baselines` last wrote must come back intact.
    #[test]
    fn baseline_persistence_round_trips() {
        let mut files = std::collections::HashMap::new();
        files.insert("a.txt".to_string(), "abc123".to_string());
        files.insert("gone.txt".to_string(), "deleted".to_string());
        files.insert("public/".to_string(), "dir".to_string());
        let mut baselines = std::collections::HashMap::new();
        baselines.insert(
            "session-1".to_string(),
            crate::state::SessionBaseline {
                head_sha: "deadbeef".to_string(),
                files,
            },
        );

        // Round-trip through the same JSON (de)serialization
        // save/load_session_baselines use, without touching the real
        // per-user data dir (keeps this test hermetic).
        let raw = serde_json::to_string_pretty(&baselines).unwrap();
        let loaded: std::collections::HashMap<String, crate::state::SessionBaseline> =
            serde_json::from_str(&raw).unwrap();

        assert_eq!(loaded.len(), 1);
        let loaded_baseline = &loaded["session-1"];
        assert_eq!(loaded_baseline.head_sha, "deadbeef");
        assert_eq!(
            loaded_baseline.files.get("a.txt").map(String::as_str),
            Some("abc123")
        );
        assert_eq!(
            loaded_baseline.files.get("gone.txt").map(String::as_str),
            Some("deleted")
        );
        assert_eq!(
            loaded_baseline.files.get("public/").map(String::as_str),
            Some("dir")
        );
    }
}

/// Commit-center git-mutation commands (`git_commit_paths`,
/// `git_commit_and_push`, `git_create_branch_and_commit`, `git_create_pr`)
/// plus `write_temp_blob`. These are `#[tauri::command]`s that take
/// `State<'_, AppState>`, so the harness below builds a real (mocked-runtime)
/// `tauri::App`, `.manage()`s an `AppState` wired to an in-memory
/// `EngineService`, and reads the `State` back off it — the standard way to
/// unit test a Tauri command per `tauri::test::mock_app`.
#[cfg(test)]
mod commit_center_tests {
    use std::path::Path;

    use agentloop_core::{Agent, AgentError, EventStream};
    use agentloop_session::MemoryStore;
    use async_trait::async_trait;
    use tauri::Manager;

    use super::*;
    use crate::commands::write_temp_blob;

    /// Minimal git repo fixture under a fresh temp dir, scoped to this
    /// module's own tests so it has no cross-module test dependency.
    fn init_repo() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "flex-commit-center-test-{}-{}-{}",
            std::process::id(),
            nanos,
            n
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let out = crate::win_console::command("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        dir
    }

    fn write(dir: &Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn commit_all(dir: &Path, msg: &str) {
        crate::win_console::command("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
            .args(["commit", "-q", "-m", msg])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    /// `git status --porcelain` lines for a repo dir, for asserting which
    /// paths are (not) still dirty after a commit.
    fn status_lines(dir: &Path) -> Vec<String> {
        let out = crate::win_console::command("git")
            .args(["status", "--porcelain"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.to_string())
            .collect()
    }

    fn current_branch(dir: &Path) -> String {
        let out = crate::win_console::command("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Test-only `Agent` stub. None of the commit-center commands under test
    /// call anything on `Agent` (they only read `SessionMeta` via
    /// `EngineService::session_meta`, which goes straight to the
    /// `SessionStore`), so every method just panics if ever invoked — that
    /// would indicate the test started exercising a code path these tests
    /// don't intend to cover, not a real production concern.
    struct StubAgent;

    #[async_trait]
    impl Agent for StubAgent {
        fn info(&self) -> agentloop_contracts::AgentInfo {
            unimplemented!("StubAgent::info not exercised by commit-center tests")
        }

        fn capabilities(&self) -> agentloop_contracts::AgentCaps {
            unimplemented!("StubAgent::capabilities not exercised by commit-center tests")
        }

        async fn create_session(&self, _params: NewSessionParams) -> Result<SessionId, AgentError> {
            unimplemented!("StubAgent::create_session not exercised by commit-center tests")
        }

        async fn resume_session(&self, _id: &SessionId) -> Result<(), AgentError> {
            unimplemented!("StubAgent::resume_session not exercised by commit-center tests")
        }

        async fn list_sessions(&self) -> Result<Vec<SessionMeta>, AgentError> {
            unimplemented!("StubAgent::list_sessions not exercised by commit-center tests")
        }

        fn events(&self, _session: &SessionId) -> Result<EventStream, AgentError> {
            unimplemented!("StubAgent::events not exercised by commit-center tests")
        }

        async fn prompt(
            &self,
            _session: &SessionId,
            _input: PromptInput,
            _opts: TurnOptions,
        ) -> Result<TurnSummary, AgentError> {
            unimplemented!("StubAgent::prompt not exercised by commit-center tests")
        }

        async fn cancel(&self, _session: &SessionId) -> Result<(), AgentError> {
            unimplemented!("StubAgent::cancel not exercised by commit-center tests")
        }

        async fn respond_permission(
            &self,
            _session: &SessionId,
            _id: PermissionRequestId,
            _decision: PermissionDecision,
        ) -> Result<(), AgentError> {
            unimplemented!("StubAgent::respond_permission not exercised by commit-center tests")
        }
    }

    /// Builds an `AppState` whose `EngineService` has exactly one session
    /// (id `"s1"`), pointed at `cwd`, non-isolated (`base_cwd: None`) — the
    /// shape every commit-center command expects for a plain (non-isolated)
    /// repo session.
    fn state_with_session(cwd: &Path) -> AppState {
        let session_store: Arc<dyn agentloop_core::SessionStore> = Arc::new(MemoryStore::new());
        let now = agentloop_contracts::now_ms();
        let meta = SessionMeta {
            id: SessionId::from("s1".to_string()),
            title: None,
            agent_id: "native".to_string(),
            parent_id: None,
            role: None,
            depth: 0,
            provider_session_id: None,
            cwd: cwd.to_path_buf(),
            model: None,
            fallback_models: Vec::new(),
            mode: None,
            isolation: None,
            workspace_id: None,
            executor: None,
            base_cwd: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        // Seed synchronously. `MemoryStore::create` never actually awaits
        // (it just locks a std Mutex), so a bare `futures::executor::block_on`
        // resolves it immediately without needing (or conflicting with) a
        // tokio runtime — this helper is called from inside `#[tokio::test]`
        // bodies, where spinning up a second tokio runtime would panic.
        futures::executor::block_on(session_store.create(meta)).expect("seed session meta");

        let engine = EngineService::new(Arc::new(StubAgent), session_store);

        let jsonl_dir = std::env::temp_dir().join(format!(
            "flex-commit-center-jsonl-{}-{}",
            std::process::id(),
            agentloop_contracts::now_ms()
        ));
        let jsonl_store =
            Arc::new(agentloop_session::JsonlStore::open(&jsonl_dir).expect("open jsonl store"));

        AppState::new(jsonl_store, ProviderConfig::default(), Some(engine))
    }

    /// Wraps `state_with_session` in a mocked-runtime Tauri app and hands
    /// back a real `State<AppState>`, the only way `#[tauri::command]`
    /// functions taking `State` can be called directly from a unit test.
    fn mock_state_with_session(cwd: &Path) -> tauri::App<tauri::test::MockRuntime> {
        let app_state = state_with_session(cwd);
        tauri::test::mock_builder()
            .manage(app_state)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build mock app")
    }

    #[tokio::test]
    async fn git_commit_paths_stages_only_selected_paths() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        write(&dir, "b.txt", "v1\n");
        commit_all(&dir, "initial commit");

        // Two files get dirtied; only one is selected for commit.
        write(&dir, "a.txt", "v2 (to be committed)\n");
        write(&dir, "b.txt", "v2 (left dirty)\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let sha = git_commit_paths(
            state,
            "s1".to_string(),
            "commit a only".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect("commit should succeed");
        assert!(!sha.is_empty());

        let status = status_lines(&dir);
        assert!(
            status.iter().all(|l| !l.contains("a.txt")),
            "a.txt must no longer be dirty after being committed: {status:?}"
        );
        assert!(
            status.iter().any(|l| l.contains("b.txt")),
            "b.txt must still be dirty (not selected for commit): {status:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_paths_rejects_empty_message() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let err = git_commit_paths(
            state,
            "s1".to_string(),
            "   ".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect_err("empty/whitespace-only message must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_paths_rejects_empty_paths() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let err = git_commit_paths(state, "s1".to_string(), "msg".to_string(), vec![])
            .await
            .expect_err("empty path list must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_and_push_commits_locally_when_no_remote() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        // No remote configured: the commit step must still land even though
        // the push step is guaranteed to fail.
        let err = git_commit_and_push(
            state,
            "s1".to_string(),
            "commit then push".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect_err("push must fail with no remote configured");
        assert!(matches!(err, DesktopError::Message(_)));

        let log = crate::win_console::command("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let log_text = String::from_utf8_lossy(&log.stdout);
        assert!(
            log_text.contains("commit then push"),
            "commit must have landed locally even though push failed: {log_text}"
        );
        assert!(
            status_lines(&dir).iter().all(|l| !l.contains("a.txt")),
            "a.txt must be committed (clean) despite the push failure"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_commit_and_push_pushes_new_branch_to_bare_remote_with_no_upstream() {
        // A local bare repo stands in for `origin` so the real push +
        // no-upstream `-u` retry path in `push_current_branch` gets
        // exercised end to end, not just the local-commit half.
        let remote_dir = init_repo();
        // `git init` alone can't produce a bare repo via our helper, so
        // reinitialize as bare directly.
        std::fs::remove_dir_all(&remote_dir).ok();
        std::fs::create_dir_all(&remote_dir).unwrap();
        let out = crate::win_console::command("git")
            .args(["init", "--bare", "-q"])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        assert!(out.status.success());

        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        crate::win_console::command("git")
            .args(["remote", "add", "origin"])
            .arg(&remote_dir)
            .current_dir(&dir)
            .output()
            .unwrap();
        // Push the initial commit once first so the bare remote has the
        // branch's history; still no upstream tracking ref is set, so the
        // *next* push exercises the "no upstream" -u retry path.
        crate::win_console::command("git")
            .args(["push", "origin", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap();

        write(&dir, "a.txt", "v2 (to push)\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let sha = git_commit_and_push(
            state,
            "s1".to_string(),
            "push to bare remote".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect("commit+push against a real bare remote should succeed");
        assert!(!sha.is_empty());

        let branch = current_branch(&dir);
        let remote_log = crate::win_console::command("git")
            .args(["log", "--oneline", "-1", &branch])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        let remote_log_text = String::from_utf8_lossy(&remote_log.stdout);
        assert!(
            remote_log_text.contains("push to bare remote"),
            "bare remote must have received the pushed commit: {remote_log_text}"
        );

        // Upstream tracking must now be set (the `-u` retry path ran).
        let upstream = crate::win_console::command("git")
            .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            upstream.status.success(),
            "an upstream tracking branch must be configured after the -u retry: {}",
            String::from_utf8_lossy(&upstream.stderr)
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&remote_dir).ok();
    }

    #[tokio::test]
    async fn git_create_branch_and_commit_creates_and_checks_out_branch() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let sha = git_create_branch_and_commit(
            state,
            "s1".to_string(),
            "feature/my-branch".to_string(),
            "commit on new branch".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect("branch create + commit should succeed");
        assert!(!sha.is_empty());

        assert_eq!(current_branch(&dir), "feature/my-branch");

        let branches = crate::win_console::command("git")
            .args(["branch", "--list", "feature/my-branch"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&branches.stdout).contains("feature/my-branch"),
            "new branch must exist in the branch list"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_create_branch_and_commit_rejects_invalid_branch_name() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2\n");
        let branch_before = current_branch(&dir);

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        let err = git_create_branch_and_commit(
            state,
            "s1".to_string(),
            "".to_string(),
            "msg".to_string(),
            vec!["a.txt".to_string()],
        )
        .await
        .expect_err("empty branch name must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));

        // Must not have switched branches on the rejected attempt.
        assert_eq!(current_branch(&dir), branch_before);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_create_pr_degrades_when_gh_is_unavailable() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2 (to push)\n");

        // A bare repo stands in for `origin` so the push half of
        // `git_create_pr` has somewhere to succeed against — `gh` itself is
        // hidden via PATH below, which is what should trigger the degraded
        // (non-error) outcome.
        let remote_dir = init_repo();
        std::fs::remove_dir_all(&remote_dir).ok();
        std::fs::create_dir_all(&remote_dir).unwrap();
        crate::win_console::command("git")
            .args(["init", "--bare", "-q"])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
            .args(["remote", "add", "origin"])
            .arg(&remote_dir)
            .current_dir(&dir)
            .output()
            .unwrap();
        crate::win_console::command("git")
            .args(["push", "-u", "origin", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap();
        write(&dir, "a.txt", "v3 (to push again)\n");

        let app = mock_state_with_session(&dir);
        let state: State<'_, AppState> = app.state();

        // Simulate "gh absent" by restricting PATH for this process to just
        // wherever `git` itself resolves from (so `git add`/`commit`/`push`
        // still work), excluding every other PATH entry — in particular
        // wherever `gh` lives (e.g. Homebrew's bin). `git_create_pr` shells
        // out to `gh auth status`/`gh pr create` via `std::process::Command`,
        // which resolves through PATH, so this reliably makes gh
        // "not available" without depending on whether the host actually has
        // gh installed.
        //
        // This mutates the process-wide `PATH` env var, which is safe here
        // only because no other test in this suite invokes `gh`, and `git`
        // remains resolvable throughout the narrowed window (serialized by
        // `PATH_MUTATION_GUARD` against any future PATH-mutating test). A
        // `tokio::sync::Mutex` (not `std::sync::Mutex`) is required here
        // since the guard is held across the `git_create_pr(..).await` below.
        static PATH_MUTATION_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _path_guard = PATH_MUTATION_GUARD.lock().await;

        let git_path = crate::win_console::command("which")
            .arg("git")
            .output()
            .ok()
            .and_then(|out| {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                path.rsplit_once('/').map(|(dir, _)| dir.to_string())
            })
            .expect("resolve git's directory via `which git`");
        let original_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", &git_path);
        }

        let result = git_create_pr(
            state,
            "s1".to_string(),
            "commit for pr".to_string(),
            vec!["a.txt".to_string()],
            None,
            None,
        )
        .await;

        unsafe {
            std::env::set_var("PATH", original_path);
        }

        let outcome = result.expect("degraded gh path must still return Ok, not Err");
        assert!(!outcome.commit_sha.is_empty());
        assert!(outcome.pr_url.is_none());
        assert!(
            outcome.degraded_reason.is_some(),
            "missing gh must surface a degraded_reason rather than silently succeeding"
        );

        // The commit + push must have gone through even though the PR step
        // was skipped.
        let remote_log = crate::win_console::command("git")
            .args(["log", "--oneline", "-1"])
            .current_dir(&remote_dir)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&remote_log.stdout).contains("commit for pr"),
            "push must have landed on the remote despite gh being unavailable"
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&remote_dir).ok();
    }

    #[test]
    fn write_temp_blob_accepts_allowlisted_extensions() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        for ext in ["png", "jpg", "jpeg", "gif", "webp"] {
            let path = rt
                .block_on(write_temp_blob(vec![1, 2, 3, 4], ext.to_string()))
                .unwrap_or_else(|e| panic!("ext `{ext}` should be accepted: {e}"));
            assert!(
                Path::new(&path).exists(),
                "returned path must exist on disk: {path}"
            );
            let contents = std::fs::read(&path).unwrap();
            assert_eq!(contents, vec![1, 2, 3, 4]);
            std::fs::remove_file(&path).ok();
        }
    }

    #[tokio::test]
    async fn write_temp_blob_rejects_disallowed_extension() {
        let err = write_temp_blob(vec![1, 2, 3], "exe".to_string())
            .await
            .expect_err("non-image extension must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));
    }

    #[tokio::test]
    async fn write_temp_blob_rejects_empty_bytes() {
        let err = write_temp_blob(Vec::new(), "png".to_string())
            .await
            .expect_err("empty blob must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));
    }

    #[tokio::test]
    async fn write_temp_blob_rejects_oversized_blob() {
        // Just over the 20MB cap — one byte over is enough to prove the
        // boundary check, no need to allocate something wastefully larger.
        let bytes = vec![0u8; 20 * 1024 * 1024 + 1];
        let err = write_temp_blob(bytes, "png".to_string())
            .await
            .expect_err("oversized blob must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));
    }
}
