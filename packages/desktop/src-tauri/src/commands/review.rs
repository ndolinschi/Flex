
use super::common::{base_head_sha, porcelain_code, resolve_review_path, review_dirs};
use super::git::diff_against_rev;
use super::prelude::*;

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_undo_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<()> {
    let (dir, base_cwd) = review_dirs(&state, &session_id).await?;
    let path = resolve_review_path(&path, &dir)?;

    if let Some(code) = porcelain_code(&dir, &path)? {
        if code == "??" {
            let full = dir.join(&path);
            return std::fs::remove_file(&full).map_err(|e| {
                DesktopError::Message(format!(
                    "failed to delete untracked file `{}`: {e}",
                    full.display()
                ))
            });
        }
    }

    let checkout_from = |rev: &str| -> DesktopResult<std::process::Output> {
        crate::win_console::command("git")
            .args(["-C"])
            .arg(&dir)
            .args(["checkout", rev, "--", &path])
            .output()
            .map_err(|e| {
                DesktopError::Message(format!(
                    "git checkout {rev} -- {path} failed in `{}`: {e}",
                    dir.display()
                ))
            })
    };

    if let Some(base_dir) = &base_cwd {
        let base_head = base_head_sha(base_dir)?;
        let out = checkout_from(&base_head)?;
        if out.status.success() {
            return Ok(());
        }
        let fallback = checkout_from("HEAD")?;
        if fallback.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&fallback.stderr).trim().to_string();
        return Err(DesktopError::Message(format!(
            "failed to revert `{path}` in `{}`: {}",
            dir.display(),
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )));
    }

    let out = checkout_from("HEAD")?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(DesktopError::Message(format!(
        "failed to revert `{path}` in `{}`: {}",
        dir.display(),
        if stderr.is_empty() {
            "unknown error".to_string()
        } else {
            stderr
        }
    )))
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_keep_file(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<()> {
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;
    let path = resolve_review_path(&path, &worktree)?;
    let Some(base_dir) = base_cwd else {
        return Err(DesktopError::Message("session is not isolated".into()));
    };

    let src = worktree.join(&path);
    let dst = base_dir.join(&path);

    if src.exists() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                DesktopError::Message(format!(
                    "failed to create directory `{}`: {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::copy(&src, &dst).map_err(|e| {
            DesktopError::Message(format!(
                "failed to copy `{}` to `{}`: {e}",
                src.display(),
                dst.display()
            ))
        })?;
        Ok(())
    } else {
        match std::fs::remove_file(&dst) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DesktopError::Message(format!(
                "failed to remove `{}`: {e}",
                dst.display()
            ))),
        }
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_apply_patch(
    state: State<'_, AppState>,
    session_id: String,
    patch: String,
    target: String,
    reverse: bool,
) -> DesktopResult<()> {
    if patch.trim().is_empty() {
        return Err(DesktopError::Message("patch is empty".into()));
    }
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;
    let dir = match target.as_str() {
        "worktree" => worktree,
        "base" => base_cwd.ok_or_else(|| {
            DesktopError::Message("session is not isolated — no base directory".into())
        })?,
        other => {
            return Err(DesktopError::Message(format!(
                "unknown patch target: {other} (expected \"worktree\" or \"base\")"
            )));
        }
    };

    let file_name = format!(
        "flex-review-patch-{}-{}.diff",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let patch_path = std::env::temp_dir().join(file_name);
    std::fs::write(&patch_path, &patch).map_err(|e| {
        DesktopError::Message(format!(
            "failed to write temp patch file `{}`: {e}",
            patch_path.display()
        ))
    })?;

    let mut args: Vec<&str> = vec!["-C"];
    let dir_str = dir.to_string_lossy();
    args.push(&dir_str);
    args.push("apply");
    if reverse {
        args.push("--reverse");
    }
    args.push("--whitespace=nowarn");
    let patch_path_str = patch_path.to_string_lossy();
    args.push(&patch_path_str);

    let result = crate::win_console::command("git").args(&args).output();

    let cleanup = std::fs::remove_file(&patch_path);
    if let Err(e) = cleanup {
        tracing::warn!(path = %patch_path.display(), error = %e, "failed to remove temp patch file");
    }

    let out = result.map_err(|e| DesktopError::Message(format!("git apply failed: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(DesktopError::Message(format!(
            "patch failed: {}",
            if stderr.is_empty() {
                "unknown error".to_string()
            } else {
                stderr
            }
        )))
    }
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn review_file_diff(
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> DesktopResult<String> {
    let (worktree, base_cwd) = review_dirs(&state, &session_id).await?;
    let path = resolve_review_path(&path, &worktree)?;

    let base_head = match &base_cwd {
        Some(base_dir) => base_head_sha(base_dir)?,
        None => "HEAD".to_string(),
    };
    diff_against_rev(&worktree, &base_head, &path)
}
