use std::path::{Path, PathBuf};

use agentloop_core::ToolError;

pub(crate) fn require_absolute(raw: &str, cwd: &Path) -> Result<PathBuf, ToolError> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(ToolError::InvalidInput(format!(
            "`file_path` must be an absolute path, but got `{raw}`. \
             The session working directory is `{cwd}`. \
             For a file there, pass `{suggested}`.",
            cwd = cwd.display(),
            suggested = cwd.join(raw).display(),
        )))
    }
}

pub(crate) async fn ensure_parent(path: &Path, create_dirs: bool) -> Result<(), ToolError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() || tokio::fs::metadata(parent).await.is_ok() {
        return Ok(());
    }
    if create_dirs {
        tokio::fs::create_dir_all(parent).await.map_err(|err| {
            ToolError::Execution(format!(
                "Cannot create parent directory `{}`: {err}.",
                parent.display()
            ))
        })?;
        return Ok(());
    }
    Err(ToolError::Execution(format!(
        "Parent directory `{}` does not exist. \
         Create it first or retry with `create_dirs: true`.",
        parent.display()
    )))
}

pub(crate) fn relative_from(abs_path: &Path, cwd: &Path) -> String {
    abs_path
        .strip_prefix(cwd)
        .map(|rel| rel.display().to_string())
        .unwrap_or_else(|_| abs_path.display().to_string())
}
