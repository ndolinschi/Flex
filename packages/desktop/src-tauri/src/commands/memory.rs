
use super::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryDto {
    pub id: String,
    pub title: String,
    pub content: Option<String>,
    pub updated_at_ms: Option<u64>,
    pub expires_at_ms: Option<u64>,
}

pub(crate) fn memory_dir() -> DesktopResult<PathBuf> {
    agentloop_sdk::learning::default_memory_dir()
        .ok_or_else(|| DesktopError::Message("could not resolve home directory".into()))
}

pub(crate) fn validate_memory_id(id: &str) -> DesktopResult<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("memory id is required".into()));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.chars().any(char::is_whitespace) {
        return Err(DesktopError::Message(
            "memory id must not contain slashes or whitespace".into(),
        ));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(DesktopError::Message("invalid memory id".into()));
    }
    Ok(trimmed)
}

pub(crate) fn memory_title(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_owned())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Untitled memory".to_owned())
}

pub(crate) fn modified_ms(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

const EXPIRY_SIDECAR_FILE: &str = "expiry.json";

type ExpiryMap = std::collections::BTreeMap<String, u64>;

pub(crate) fn expiry_sidecar_path(dir: &std::path::Path) -> PathBuf {
    dir.join(EXPIRY_SIDECAR_FILE)
}

pub(crate) fn read_expiry_map(dir: &std::path::Path) -> ExpiryMap {
    let path = expiry_sidecar_path(dir);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return ExpiryMap::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub(crate) fn write_expiry_map(dir: &std::path::Path, map: &ExpiryMap) -> DesktopResult<()> {
    let path = expiry_sidecar_path(dir);
    if map.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => {
                return Err(DesktopError::Message(format!(
                    "cannot remove `{}`: {e}",
                    path.display()
                )));
            }
        }
    }
    let json = serde_json::to_string_pretty(map)
        .map_err(|e| DesktopError::Message(format!("cannot serialize expiry map: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| DesktopError::Message(format!("cannot write `{}`: {e}", path.display())))
}

pub(crate) fn set_expiry_in_dir(
    dir: &std::path::Path,
    id: &str,
    expires_at_ms: Option<u64>,
) -> DesktopResult<()> {
    let mut map = read_expiry_map(dir);
    match expires_at_ms {
        Some(ts) => {
            map.insert(id.to_owned(), ts);
        }
        None => {
            map.remove(id);
        }
    }
    write_expiry_map(dir, &map)
}

pub(crate) fn purge_expired_memories(dir: &std::path::Path) {
    let mut map = read_expiry_map(dir);
    if map.is_empty() {
        return;
    }
    let now = now_ms();
    let expired: Vec<String> = map
        .iter()
        .filter(|(_, &ts)| ts <= now)
        .map(|(id, _)| id.clone())
        .collect();
    if expired.is_empty() {
        return;
    }
    for id in &expired {
        let path = dir.join(format!("{id}.md"));
        let _ = std::fs::remove_file(&path);
        map.remove(id);
    }
    let _ = write_expiry_map(dir, &map);
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn memory_list() -> DesktopResult<Vec<MemoryEntryDto>> {
    let dir = memory_dir()?;
    purge_expired_memories(&dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot read memory directory `{}`: {e}",
                dir.display()
            )));
        }
    };

    let expiry = read_expiry_map(&dir);
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") || !path.is_file() {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let updated_at_ms = std::fs::metadata(&path).ok().and_then(|m| modified_ms(&m));
        out.push(MemoryEntryDto {
            id: id.to_owned(),
            title: memory_title(&body),
            content: None,
            updated_at_ms,
            expires_at_ms: expiry.get(id).copied(),
        });
    }
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms).then(a.id.cmp(&b.id)));
    Ok(out)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn memory_get(id: String) -> DesktopResult<MemoryEntryDto> {
    let id = validate_memory_id(&id)?;
    let dir = memory_dir()?;
    let path = dir.join(format!("{id}.md"));
    let body = tokio::fs::read_to_string(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DesktopError::Message(format!("memory `{id}` not found"))
        } else {
            DesktopError::Message(format!("cannot read memory `{id}`: {e}"))
        }
    })?;
    let updated_at_ms = tokio::fs::metadata(&path)
        .await
        .ok()
        .and_then(|m| modified_ms(&m));
    let expires_at_ms = read_expiry_map(&dir).get(id).copied();
    Ok(MemoryEntryDto {
        id: id.to_owned(),
        title: memory_title(&body),
        content: Some(body),
        updated_at_ms,
        expires_at_ms,
    })
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn memory_remove(id: String) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = memory_dir()?;
    let path = dir.join(format!("{id}.md"));
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot delete memory `{id}`: {e}"
            )));
        }
    }
    set_expiry_in_dir(&dir, id, None)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn memory_set_expiry(id: String, expires_at_ms: Option<u64>) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = memory_dir()?;
    set_expiry_in_dir(&dir, id, expires_at_ms)
}

pub(crate) fn project_memory_dir(cwd: &str) -> DesktopResult<PathBuf> {
    let trimmed = cwd.trim();
    if trimmed.is_empty() {
        return Err(DesktopError::Message("cwd is required".into()));
    }
    let path = PathBuf::from(trimmed);
    let canonical = path
        .canonicalize()
        .map_err(|e| DesktopError::Message(format!("invalid cwd `{trimmed}`: {e}")))?;
    if !canonical.is_dir() {
        return Err(DesktopError::Message(format!(
            "cwd `{trimmed}` is not a directory"
        )));
    }
    Ok(canonical.join(".agent").join("memory"))
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn project_memory_list(cwd: String) -> DesktopResult<Vec<MemoryEntryDto>> {
    let dir = project_memory_dir(&cwd)?;
    purge_expired_memories(&dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot read memory directory `{}`: {e}",
                dir.display()
            )));
        }
    };

    let expiry = read_expiry_map(&dir);
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") || !path.is_file() {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let body = std::fs::read_to_string(&path).unwrap_or_default();
        let updated_at_ms = std::fs::metadata(&path).ok().and_then(|m| modified_ms(&m));
        out.push(MemoryEntryDto {
            id: id.to_owned(),
            title: memory_title(&body),
            content: None,
            updated_at_ms,
            expires_at_ms: expiry.get(id).copied(),
        });
    }
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms).then(a.id.cmp(&b.id)));
    Ok(out)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn project_memory_get(cwd: String, id: String) -> DesktopResult<MemoryEntryDto> {
    let id = validate_memory_id(&id)?;
    let dir = project_memory_dir(&cwd)?;
    let path = dir.join(format!("{id}.md"));
    let body = tokio::fs::read_to_string(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DesktopError::Message(format!("memory `{id}` not found"))
        } else {
            DesktopError::Message(format!("cannot read memory `{id}`: {e}"))
        }
    })?;
    let updated_at_ms = tokio::fs::metadata(&path)
        .await
        .ok()
        .and_then(|m| modified_ms(&m));
    let expires_at_ms = read_expiry_map(&dir).get(id).copied();
    Ok(MemoryEntryDto {
        id: id.to_owned(),
        title: memory_title(&body),
        content: Some(body),
        updated_at_ms,
        expires_at_ms,
    })
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn project_memory_remove(cwd: String, id: String) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = project_memory_dir(&cwd)?;
    let path = dir.join(format!("{id}.md"));
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(DesktopError::Message(format!(
                "cannot delete memory `{id}`: {e}"
            )));
        }
    }
    set_expiry_in_dir(&dir, id, None)
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn project_memory_set_expiry(
    cwd: String,
    id: String,
    expires_at_ms: Option<u64>,
) -> DesktopResult<()> {
    let id = validate_memory_id(&id)?;
    let dir = project_memory_dir(&cwd)?;
    set_expiry_in_dir(&dir, id, expires_at_ms)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserIdentityDto {
    pub name: String,
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn user_identity(_state: State<'_, AppState>) -> DesktopResult<UserIdentityDto> {
    let git_name = crate::win_console::command("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let name = git_name
        .or_else(|| std::env::var("USER").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "User".to_string());

    Ok(UserIdentityDto { name })
}
