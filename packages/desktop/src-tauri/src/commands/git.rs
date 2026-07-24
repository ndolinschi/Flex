
use super::common::{require_service, review_dirs, validate_repo_relative_path};
use super::prelude::*;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Short TTL for `git_is_repo` probes — the FE hits this often on cwd changes.
const GIT_IS_REPO_TTL: Duration = Duration::from_secs(30);

fn git_is_repo_cache() -> &'static Mutex<HashMap<String, (Instant, bool)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn git_is_repo_cached(cwd: &str) -> Option<bool> {
    let guard = git_is_repo_cache().lock().ok()?;
    let (at, ok) = guard.get(cwd)?;
    if at.elapsed() < GIT_IS_REPO_TTL {
        Some(*ok)
    } else {
        None
    }
}

fn git_is_repo_store(cwd: String, ok: bool) {
    if let Ok(mut guard) = git_is_repo_cache().lock() {
        // Bound cache size so a long-lived app probing many paths can't grow forever.
        if guard.len() > 256 {
            guard.clear();
        }
        guard.insert(cwd, (Instant::now(), ok));
    }
}

pub(crate) fn git_is_repo_sync(cwd: &str) -> bool {
    crate::win_console::command("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(cwd)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

pub(crate) fn git_has_remote_sync(cwd: &str) -> bool {
    crate::win_console::command("git")
        .args(["remote"])
        .current_dir(cwd)
        .output()
        .map(|out| out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
        .unwrap_or(false)
}

pub(crate) fn git_branch_sync(cwd: &str) -> Option<String> {
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

pub(crate) fn git_list_branches_sync(cwd: &str) -> DesktopResult<Vec<String>> {
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

#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub async fn git_is_repo(cwd: String) -> bool {
    if let Some(cached) = git_is_repo_cached(&cwd) {
        return cached;
    }
    match tokio::task::spawn_blocking(move || {
        let ok = git_is_repo_sync(&cwd);
        git_is_repo_store(cwd, ok);
        ok
    })
    .await
    {
        Ok(ok) => ok,
        Err(err) => {
            tracing::warn!(error = %err, "git_is_repo join failed");
            false
        }
    }
}

#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub async fn git_has_remote(cwd: String) -> bool {
    match tokio::task::spawn_blocking(move || git_has_remote_sync(&cwd)).await {
        Ok(ok) => ok,
        Err(err) => {
            tracing::warn!(error = %err, "git_has_remote join failed");
            false
        }
    }
}

#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub async fn git_branch(cwd: String) -> Option<String> {
    match tokio::task::spawn_blocking(move || git_branch_sync(&cwd)).await {
        Ok(branch) => branch,
        Err(err) => {
            tracing::warn!(error = %err, "git_branch join failed");
            None
        }
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_list_branches(cwd: String) -> DesktopResult<Vec<String>> {
    tokio::task::spawn_blocking(move || git_list_branches_sync(&cwd))
        .await
        .map_err(|e| DesktopError::Message(format!("git list branches join: {e}")))?
}

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
    pub path: String,
    pub status: String,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

const MAX_STATUS_FILES: usize = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusSummary {
    pub files: Vec<GitFileStatus>,
    pub total_count: usize,
    pub total_added: u32,
    pub total_removed: u32,
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_status(cwd: String) -> DesktopResult<GitStatusSummary> {
    tokio::task::spawn_blocking(move || Ok(summarize(git_status_full(&cwd)?)))
        .await
        .map_err(|e| DesktopError::Message(format!("git status join: {e}")))?
}

pub(crate) fn git_status_full(cwd: &str) -> DesktopResult<Vec<GitFileStatus>> {
    let porcelain = match crate::win_console::command("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => return Ok(Vec::new()),
    };

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
        if let Some((_, new)) = path.split_once(" -> ") {
            path = new.trim().to_string();
        }
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

async fn compute_status_since_baseline(
    state: &AppState,
    session_id: &str,
) -> DesktopResult<GitStatusSummary> {
    let (cwd, base_cwd) = review_dirs(state, session_id).await?;
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
            .get(session_id)
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_status_since_baseline(
    state: State<'_, AppState>,
    session_id: String,
) -> DesktopResult<GitStatusSummary> {
    compute_status_since_baseline(state.inner(), &session_id).await
}

/// One entry in a multi-session git status batch (sidebar badges).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusBatchEntry {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<GitStatusSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const MAX_STATUS_BATCH: usize = 64;
/// Cap concurrent git subprocesses so large sidebars don't thrash the disk.
const STATUS_BATCH_CONCURRENCY: usize = 4;

/// Single IPC for many sessions — avoids N× round-trips from the sidebar poller.
/// Sessions run in chunks of `STATUS_BATCH_CONCURRENCY` via `join_all`.
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_status_since_baseline_batch(
    state: State<'_, AppState>,
    session_ids: Vec<String>,
) -> DesktopResult<Vec<GitStatusBatchEntry>> {
    let mut ids: Vec<String> = session_ids
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .take(MAX_STATUS_BATCH)
        .collect();
    ids.sort();
    ids.dedup();

    let mut out = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(STATUS_BATCH_CONCURRENCY) {
        let results =
            futures::future::join_all(chunk.iter().map(|id| {
                compute_status_since_baseline(state.inner(), id)
            }))
            .await;
        for (id, result) in chunk.iter().zip(results) {
            match result {
                Ok(summary) => out.push(GitStatusBatchEntry {
                    session_id: id.clone(),
                    summary: Some(summary),
                    error: None,
                }),
                Err(err) => out.push(GitStatusBatchEntry {
                    session_id: id.clone(),
                    summary: None,
                    error: Some(err.to_string()),
                }),
            }
        }
    }
    Ok(out)
}

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

pub(crate) fn hash_objects_batch(
    cwd: &std::path::Path,
    paths: &[String],
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    if paths.is_empty() {
        return out;
    }

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

pub(crate) fn capture_session_baseline(
    cwd: &std::path::Path,
) -> Option<crate::state::SessionBaseline> {
    if !cwd.is_dir() {
        return None;
    }
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_diff(cwd: String, path: String) -> DesktopResult<String> {
    let path = path.trim();
    if path.is_empty() || path.starts_with('-') {
        return Err(DesktopError::Message("invalid path".into()));
    }
    diff_against_rev(std::path::Path::new(&cwd), "HEAD", path)
}

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
    if stderr.contains("has no upstream branch") || stderr.contains("--set-upstream") {
        let branch = git_branch_sync(&cwd.to_string_lossy())
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
    pub degraded_reason: Option<String>,
}

fn gh_bin_cache() -> &'static std::sync::Mutex<Option<(std::time::Instant, bool)>> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<Option<(std::time::Instant, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn gh_bin_available() -> bool {
    use std::time::{Duration, Instant};

    const TTL: Duration = Duration::from_secs(120);

    if let Ok(guard) = gh_bin_cache().lock() {
        if let Some((at, ok)) = *guard {
            if at.elapsed() < TTL {
                return ok;
            }
        }
    }

    let ok = crate::win_console::command("gh")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);

    if let Ok(mut guard) = gh_bin_cache().lock() {
        *guard = Some((Instant::now(), ok));
    }
    ok
}

fn invalidate_gh_bin_cache() {
    if let Ok(mut guard) = gh_bin_cache().lock() {
        *guard = None;
    }
}

pub(crate) fn gh_available(_cwd: &std::path::Path) -> bool {
    gh_bin_available()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchPrInfo {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
    pub checks_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchPrStatus {
    pub gh_available: bool,
    pub pr: Option<BranchPrInfo>,
}

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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn git_pr_status(cwd: String) -> DesktopResult<BranchPrStatus> {
    tokio::task::spawn_blocking(move || git_pr_status_sync(cwd))
        .await
        .map_err(|e| DesktopError::Message(format!("git_pr_status worker failed: {e}")))?
}

fn git_pr_status_sync(cwd: String) -> DesktopResult<BranchPrStatus> {
    let path = std::path::PathBuf::from(&cwd);
    if !gh_available(&path) {
        return Ok(BranchPrStatus {
            gh_available: false,
            pr: None,
        });
    }

    let out = match crate::win_console::command("gh")
        .args([
            "pr",
            "view",
            "--json",
            "number,title,url,state,statusCheckRollup",
        ])
        .current_dir(&path)
        .output()
    {
        Ok(out) => out,
        Err(err) => {
            tracing::debug!(error = %err, "gh pr view spawn failed; treating as unavailable");
            invalidate_gh_bin_cache();
            return Ok(BranchPrStatus {
                gh_available: false,
                pr: None,
            });
        }
    };

    if !out.status.success() {
        return Ok(BranchPrStatus {
            gh_available: true,
            pr: None,
        });
    }

    let raw = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = match serde_json::from_str(raw.trim()) {
        Ok(v) => v,
        Err(err) => {
            tracing::debug!(error = %err, "gh pr view returned invalid JSON; treating as no PR");
            return Ok(BranchPrStatus {
                gh_available: true,
                pr: None,
            });
        }
    };

    let Some(number) = value.get("number").and_then(|v| v.as_u64()) else {
        return Ok(BranchPrStatus {
            gh_available: true,
            pr: None,
        });
    };
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

/// Paths changed in the open PR (for paged PR review UI).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_pr_files(cwd: String) -> DesktopResult<Vec<String>> {
    let path = std::path::PathBuf::from(&cwd);
    if !gh_available(&path) {
        return Ok(Vec::new());
    }

    let out = match crate::win_console::command("gh")
        .args(["pr", "diff", "--name-only"])
        .current_dir(&path)
        .output()
    {
        Ok(out) => out,
        Err(err) => {
            tracing::debug!(error = %err, "gh pr diff --name-only spawn failed");
            invalidate_gh_bin_cache();
            return Ok(Vec::new());
        }
    };

    if !out.status.success() {
        return Ok(Vec::new());
    }

    let mut files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    // Cap list so monorepo PRs stay navigable; FE pages diffs per file.
    const MAX_PR_FILES: usize = 500;
    files.truncate(MAX_PR_FILES);
    Ok(files)
}

/// Full PR diff, or a single path when `path` is set (paged review).
#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_pr_diff(cwd: String, path: Option<String>) -> DesktopResult<String> {
    let root = std::path::PathBuf::from(&cwd);
    if !gh_available(&root) {
        return Ok(String::new());
    }

    let mut args = vec!["pr".to_string(), "diff".to_string()];
    if let Some(rel) = path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        // Reject path traversal into the command args.
        if rel.contains("..") || rel.starts_with('/') || rel.starts_with('\\') {
            return Err(DesktopError::Message(
                "invalid path for pull request diff".into(),
            ));
        }
        args.push("--".into());
        args.push(rel.to_string());
    }

    let out = match crate::win_console::command("gh")
        .args(&args)
        .current_dir(&root)
        .output()
    {
        Ok(out) => out,
        Err(err) => {
            tracing::debug!(error = %err, "gh pr diff spawn failed; treating as unavailable");
            invalidate_gh_bin_cache();
            return Ok(String::new());
        }
    };

    if !out.status.success() {
        return Ok(String::new());
    }

    let mut diff = String::from_utf8_lossy(&out.stdout).into_owned();
    // Per-file diffs stay smaller; full PR still hard-capped.
    let max_chars: usize = if path.as_ref().is_some_and(|p| !p.trim().is_empty()) {
        256 * 1024
    } else {
        512_000
    };
    if diff.len() > max_chars {
        let mut cut = max_chars;
        while cut > 0 && !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
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

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub fn git_create_pr_for_branch(
    cwd: String,
    title: Option<String>,
    body: Option<String>,
) -> DesktopResult<CreatePrOutcome> {
    let path = std::path::PathBuf::from(&cwd);
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
            !git_has_remote_sync(&dir.to_string_lossy()),
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
            git_has_remote_sync(&dir.to_string_lossy()),
            "configured origin must count as a push remote"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod session_baseline_tests {
    use super::*;

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

    #[test]
    fn baseline_filters_pre_existing_dirty_file_but_keeps_new_edit() {
        let dir = init_repo();

        write(&dir, "pre_dirty.txt", "original\n");
        write(&dir, "untouched.txt", "original\n");
        commit_all(&dir, "initial commit");

        write(&dir, "pre_dirty.txt", "user's uncommitted edit\n");

        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");
        assert!(!baseline.head_sha.is_empty());
        assert!(baseline.files.contains_key("pre_dirty.txt"));

        write(&dir, "untouched.txt", "session edit\n");
        write(&dir, "session_new.txt", "brand new\n");

        let cwd_str = dir.to_string_lossy().to_string();
        let all = git_status_full(&cwd_str).expect("git_status_full should succeed");
        let all_paths: Vec<_> = all.iter().map(|f| f.path.as_str()).collect();
        assert!(all_paths.contains(&"pre_dirty.txt"));
        assert!(all_paths.contains(&"untouched.txt"));
        assert!(all_paths.contains(&"session_new.txt"));

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

    #[test]
    fn baseline_keeps_file_when_further_modified_after_capture() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");

        write(&dir, "a.txt", "v2 (dirty before session)\n");
        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");

        write(&dir, "a.txt", "v3 (session edit)\n");

        let current_hash = hash_object(&dir, "a.txt").unwrap();
        let baseline_hash = baseline.files.get("a.txt").unwrap();
        assert_ne!(
            &current_hash, baseline_hash,
            "hash must change after the session's own edit"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

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

    #[test]
    fn baseline_filters_pre_existing_untracked_dir_but_keeps_new_dir() {
        let dir = init_repo();
        write(&dir, "README.md", "hello\n");
        commit_all(&dir, "initial commit");

        write(&dir, "public/index.html", "<html></html>\n");

        let baseline = capture_session_baseline(&dir).expect("baseline capture should succeed");
        assert_eq!(
            baseline.files.get("public/").map(String::as_str),
            Some("dir"),
            "pre-existing untracked dir must be recorded with the dir sentinel: {:?}",
            baseline.files
        );

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

#[cfg(test)]
mod commit_center_tests {
    use std::path::Path;

    use agentloop_core::{Agent, AgentError, EventStream};
    use agentloop_session::MemoryStore;
    use async_trait::async_trait;
    use tauri::Manager;

    use super::*;
    use crate::commands::write_temp_blob;

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
            reuse_workspace_id: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
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
        let remote_dir = init_repo();
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

        assert_eq!(current_branch(&dir), branch_before);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_create_pr_degrades_when_gh_is_unavailable() {
        let dir = init_repo();
        write(&dir, "a.txt", "v1\n");
        commit_all(&dir, "initial commit");
        write(&dir, "a.txt", "v2 (to push)\n");

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
        let bytes = vec![0u8; 20 * 1024 * 1024 + 1];
        let err = write_temp_blob(bytes, "png".to_string())
            .await
            .expect_err("oversized blob must be rejected");
        assert!(matches!(err, DesktopError::Message(_)));
    }
}
