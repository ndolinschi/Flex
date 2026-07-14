//! Shared filesystem-tool helpers.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use agentloop_core::ToolError;

use super::FsState;

/// Derive an input schema for a tool.
pub(crate) fn schema_of<I: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(I))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

/// Parse a `file_path` argument, teaching the model to pass absolute paths.
pub(crate) fn require_absolute(raw: &str, cwd: &Path) -> Result<PathBuf, ToolError> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(ToolError::InvalidInput(format!(
            "`file_path` must be an absolute path, but got `{raw}`. The session working \
             directory is `{}`; for a file there, pass `{}`.",
            cwd.display(),
            cwd.join(raw).display()
        )))
    }
}

/// Enforce the read-before-modify discipline for an existing file.
///
/// `current_mtime` is the file's mtime right now; `tool_name` names the
/// caller (`Write` / `Edit`) so errors read naturally.
pub(crate) fn check_freshness(
    state: &FsState,
    path: &Path,
    current_mtime: SystemTime,
    tool_name: &str,
) -> Result<(), ToolError> {
    match state.recorded_mtime(path) {
        None => Err(ToolError::Execution(format!(
            "`{}` already exists but has not been Read in this session. Read it first to see \
             its current content, then retry the {tool_name}.",
            path.display()
        ))),
        Some(recorded) if recorded != current_mtime => Err(ToolError::Execution(format!(
            "`{}` has changed on disk since you last Read it. Read it again to get the current \
             content, then retry the {tool_name}.",
            path.display()
        ))),
        Some(_) => Ok(()),
    }
}

pub(crate) fn modified_time(metadata: &std::fs::Metadata) -> SystemTime {
    metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Resolve an optional search-root argument against the session cwd and
/// verify it is an existing directory.
pub(crate) async fn resolve_search_root(
    path: Option<&str>,
    cwd: &Path,
    tool_name: &str,
) -> Result<PathBuf, ToolError> {
    let base = match path {
        Some(p) => {
            let pb = PathBuf::from(p);
            if pb.is_absolute() { pb } else { cwd.join(pb) }
        }
        None => cwd.to_path_buf(),
    };
    let meta = tokio::fs::metadata(&base).await.map_err(|err| {
        ToolError::InvalidInput(format!(
            "{tool_name} search path `{}` does not exist or is not accessible: {err}. Pass an \
             existing directory (absolute, or relative to the session cwd `{}`), or omit `path` \
             to search the cwd.",
            base.display(),
            cwd.display()
        ))
    })?;
    if !meta.is_dir() {
        return Err(ToolError::InvalidInput(format!(
            "{tool_name} search path `{}` is a file, not a directory. Pass a directory to \
             search under, or omit `path` to search the session cwd.",
            base.display()
        )));
    }
    Ok(base)
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_owned(), false);
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("\n\n[... output truncated ...]");
    (out, true)
}
