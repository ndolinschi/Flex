//! Desktop Artifacts UI plugin — index of AI-created non-code deliverables.
//!
//! Persists an artifact index at `{cwd}/.flex/artifacts/index.json`
//! (one per project). Commands are consumed by the right-panel Artifacts tab.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db_plugin::normalize_project_key;
use crate::error::{DesktopError, DesktopResult};

// ── Wire types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    Presentation,
    Spreadsheet,
    Csv,
    Diagram,
    Image,
    Document,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub project_key: String,
    pub session_id: String,
    pub kind: ArtifactKind,
    pub relative_path: String,
    pub title: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvPreview {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
    pub row_count: usize,
}

// ── Kind inference ────────────────────────────────────────────────────────────

/// Infer artifact kind from a file path.  Returns `None` for code / source
/// files that should not be auto-registered.
pub fn infer_artifact_kind(path: &str) -> Option<ArtifactKind> {
    let lower = path.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");

    // Artifact-bearing parent directories add strong signal.
    let in_artifact_dir = lower.contains("/artifacts/")
        || lower.contains("/reports/")
        || lower.contains("/exports/")
        || lower.contains("/plans/");

    let kind = match ext {
        "csv" | "tsv" => Some(ArtifactKind::Csv),
        "xlsx" | "xls" | "ods" => Some(ArtifactKind::Spreadsheet),
        "pptx" | "ppt" | "key" => Some(ArtifactKind::Presentation),
        "png" | "jpg" | "jpeg" | "webp" | "gif" => Some(ArtifactKind::Image),
        "svg" | "mmd" | "dot" => Some(ArtifactKind::Diagram),
        "pdf" | "docx" => Some(ArtifactKind::Document),
        // Generic text/markdown/html in artifact dirs → document.
        "md" | "txt" | "html" | "htm" | "json" | "yaml" | "yml" | "xml" if in_artifact_dir => {
            Some(ArtifactKind::Document)
        }
        _ => None,
    };
    kind
}

// ── Index I/O ─────────────────────────────────────────────────────────────────

fn index_path(project_key: &str) -> DesktopResult<PathBuf> {
    if project_key.is_empty() {
        return Err(DesktopError::Message(
            "project folder is required to manage artifacts".into(),
        ));
    }
    // project_key IS the normalized cwd (absolute path, forward-slashes).
    // Reconstruct the OS-native path.
    let root = PathBuf::from(project_key.replace('/', std::path::MAIN_SEPARATOR_STR));
    let dir = root.join(".flex").join("artifacts");
    std::fs::create_dir_all(&dir)
        .map_err(|e| DesktopError::Message(format!("create artifacts dir: {e}")))?;
    Ok(dir.join("index.json"))
}

fn load_index(project_key: &str) -> DesktopResult<Vec<Artifact>> {
    let path = index_path(project_key)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| DesktopError::Message(format!("read artifacts index: {e}")))?;
    serde_json::from_str::<Vec<Artifact>>(&raw)
        .map_err(|e| DesktopError::Message(format!("parse artifacts index: {e}")))
}

fn save_index(project_key: &str, index: &[Artifact]) -> DesktopResult<()> {
    let path = index_path(project_key)?;
    let raw = serde_json::to_string_pretty(index)
        .map_err(|e| DesktopError::Message(format!("serialize artifacts index: {e}")))?;
    std::fs::write(&path, raw)
        .map_err(|e| DesktopError::Message(format!("write artifacts index: {e}")))
}

// ── Path helpers ──────────────────────────────────────────────────────────────

fn new_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("art-{nanos:x}")
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Simple RFC 3339 UTC timestamp without pulling in chrono.
    let d = secs / 86_400;
    let t = secs % 86_400;
    let h = t / 3600;
    let m = (t % 3600) / 60;
    let s = t % 60;
    // Civil date from days (Howard Hinnant's algorithm).
    let z = d as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let yr = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dy = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = if mo <= 2 { yr + 1 } else { yr };
    format!("{yr:04}-{mo:02}-{dy:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Reject relative paths and any `..` traversal; return `None` on rejection.
fn safe_relative_path(project_key: &str, relative_path: &str) -> Option<PathBuf> {
    let rel = Path::new(relative_path);
    // Must be relative and contain no `..` component.
    if rel.is_absolute() {
        return None;
    }
    if rel.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::Prefix(_)
        )
    }) {
        return None;
    }
    // Reconstruct absolute path.
    let root = PathBuf::from(project_key.replace('/', std::path::MAIN_SEPARATOR_STR));
    Some(root.join(rel))
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn artifacts_list(project_key: String) -> DesktopResult<Vec<Artifact>> {
    let key = normalize_project_key(&project_key);
    load_index(&key)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn artifacts_register(
    project_key: String,
    session_id: String,
    relative_path: String,
    title: Option<String>,
) -> DesktopResult<Artifact> {
    let key = normalize_project_key(&project_key);
    if key.is_empty() {
        return Err(DesktopError::Message(
            "project folder is required to register an artifact".into(),
        ));
    }
    if session_id.trim().is_empty() {
        return Err(DesktopError::Message("session_id is required".into()));
    }
    let rel = relative_path.trim().to_string();
    if rel.is_empty() {
        return Err(DesktopError::Message("relative_path is required".into()));
    }
    // Validate the path.
    if safe_relative_path(&key, &rel).is_none() {
        return Err(DesktopError::Message(
            "relative_path must not be absolute or contain '..'".into(),
        ));
    }

    let kind = infer_artifact_kind(&rel).unwrap_or(ArtifactKind::Other);
    let derived_title = title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            Path::new(&rel)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&rel)
                .to_string()
        });

    let mime_type = mime_for_kind(&kind, &rel);

    let mut index = load_index(&key).unwrap_or_default();

    // Upsert by (project_key, relative_path).
    if let Some(existing) = index.iter_mut().find(|a| a.relative_path == rel) {
        existing.session_id = session_id.trim().to_string();
        existing.kind = kind;
        existing.title = derived_title;
        existing.mime_type = mime_type;
        let saved = existing.clone();
        save_index(&key, &index)?;
        return Ok(saved);
    }

    let artifact = Artifact {
        id: new_id(),
        project_key: key.clone(),
        session_id: session_id.trim().to_string(),
        kind,
        relative_path: rel,
        title: derived_title,
        created_at: now_iso(),
        mime_type,
    };
    index.push(artifact.clone());
    save_index(&key, &index)?;
    Ok(artifact)
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn artifacts_remove(project_key: String, id: String) -> DesktopResult<()> {
    let key = normalize_project_key(&project_key);
    let mut index = load_index(&key).unwrap_or_default();
    let before = index.len();
    index.retain(|a| a.id != id);
    if index.len() < before {
        save_index(&key, &index)?;
    }
    Ok(())
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn artifacts_preview_csv(
    project_key: String,
    id: String,
    max_rows: Option<u32>,
) -> DesktopResult<CsvPreview> {
    let key = normalize_project_key(&project_key);
    let index = load_index(&key)?;
    let artifact = index
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| DesktopError::Message("artifact not found".into()))?;

    let abs = safe_relative_path(&key, &artifact.relative_path)
        .ok_or_else(|| DesktopError::Message("invalid artifact path".into()))?;

    let limit = max_rows.unwrap_or(200).clamp(1, 2000) as usize;

    // Read + parse synchronously — CSV files are expected to be small.
    let content = std::fs::read_to_string(&abs)
        .map_err(|e| DesktopError::Message(format!("read csv: {e}")))?;

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(content.as_bytes());

    let columns: Vec<String> = reader
        .headers()
        .map_err(|e| DesktopError::Message(format!("csv headers: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut truncated = false;

    for result in reader.records() {
        if rows.len() >= limit {
            truncated = true;
            break;
        }
        let record = result.map_err(|e| DesktopError::Message(format!("csv record: {e}")))?;
        rows.push(record.iter().map(str::to_string).collect());
    }

    let row_count = rows.len();
    Ok(CsvPreview {
        columns,
        rows,
        truncated,
        row_count,
    })
}

#[tauri::command]
#[tracing::instrument(level = "debug", skip_all, err)]
pub async fn artifacts_open_external(
    app: tauri::AppHandle,
    project_key: String,
    id: String,
) -> DesktopResult<()> {
    let key = normalize_project_key(&project_key);
    let index = load_index(&key)?;
    let artifact = index
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(|| DesktopError::Message("artifact not found".into()))?;

    let abs = safe_relative_path(&key, &artifact.relative_path)
        .ok_or_else(|| DesktopError::Message("invalid artifact path".into()))?;

    tauri_plugin_opener::open_path(abs, None::<&str>)
        .map_err(|e| DesktopError::Message(format!("open external: {e}")))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn mime_for_kind(kind: &ArtifactKind, path: &str) -> Option<String> {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match kind {
        ArtifactKind::Csv => Some("text/csv".into()),
        ArtifactKind::Spreadsheet => match ext.as_str() {
            "xlsx" => {
                Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into())
            }
            "xls" => Some("application/vnd.ms-excel".into()),
            "ods" => Some("application/vnd.oasis.opendocument.spreadsheet".into()),
            _ => None,
        },
        ArtifactKind::Presentation => match ext.as_str() {
            "pptx" => Some(
                "application/vnd.openxmlformats-officedocument.presentationml.presentation".into(),
            ),
            "ppt" => Some("application/vnd.ms-powerpoint".into()),
            _ => None,
        },
        ArtifactKind::Image => match ext.as_str() {
            "png" => Some("image/png".into()),
            "jpg" | "jpeg" => Some("image/jpeg".into()),
            "webp" => Some("image/webp".into()),
            "gif" => Some("image/gif".into()),
            "svg" => Some("image/svg+xml".into()),
            _ => None,
        },
        ArtifactKind::Diagram => match ext.as_str() {
            "svg" => Some("image/svg+xml".into()),
            _ => None,
        },
        ArtifactKind::Document => match ext.as_str() {
            "pdf" => Some("application/pdf".into()),
            "docx" => Some(
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
            ),
            "html" | "htm" => Some("text/html".into()),
            "md" => Some("text/markdown".into()),
            _ => None,
        },
        ArtifactKind::Other => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_tsv_detected() {
        assert_eq!(infer_artifact_kind("out.csv"), Some(ArtifactKind::Csv));
        assert_eq!(infer_artifact_kind("data.tsv"), Some(ArtifactKind::Csv));
    }

    #[test]
    fn spreadsheet_detected() {
        assert_eq!(
            infer_artifact_kind("report.xlsx"),
            Some(ArtifactKind::Spreadsheet)
        );
        assert_eq!(
            infer_artifact_kind("REPORT.XLS"),
            Some(ArtifactKind::Spreadsheet)
        );
    }

    #[test]
    fn presentation_detected() {
        assert_eq!(
            infer_artifact_kind("deck.pptx"),
            Some(ArtifactKind::Presentation)
        );
        assert_eq!(
            infer_artifact_kind("keynote.key"),
            Some(ArtifactKind::Presentation)
        );
    }

    #[test]
    fn image_detected() {
        assert_eq!(infer_artifact_kind("banner.png"), Some(ArtifactKind::Image));
        assert_eq!(infer_artifact_kind("photo.jpg"), Some(ArtifactKind::Image));
        assert_eq!(infer_artifact_kind("icon.webp"), Some(ArtifactKind::Image));
    }

    #[test]
    fn diagram_detected() {
        assert_eq!(infer_artifact_kind("flow.mmd"), Some(ArtifactKind::Diagram));
        assert_eq!(
            infer_artifact_kind("graph.dot"),
            Some(ArtifactKind::Diagram)
        );
        assert_eq!(infer_artifact_kind("logo.svg"), Some(ArtifactKind::Diagram));
    }

    #[test]
    fn document_detected() {
        assert_eq!(
            infer_artifact_kind("spec.pdf"),
            Some(ArtifactKind::Document)
        );
        assert_eq!(
            infer_artifact_kind("notes.docx"),
            Some(ArtifactKind::Document)
        );
    }

    #[test]
    fn code_files_not_detected() {
        assert_eq!(infer_artifact_kind("main.ts"), None);
        assert_eq!(infer_artifact_kind("lib.rs"), None);
        assert_eq!(infer_artifact_kind("mod.py"), None);
        assert_eq!(infer_artifact_kind("index.js"), None);
        assert_eq!(infer_artifact_kind("component.tsx"), None);
        assert_eq!(infer_artifact_kind("main.go"), None);
    }

    #[test]
    fn artifact_dir_promotes_generic_files() {
        assert_eq!(
            infer_artifact_kind("reports/summary.md"),
            Some(ArtifactKind::Document)
        );
        assert_eq!(
            infer_artifact_kind("exports/data.json"),
            Some(ArtifactKind::Document)
        );
        // Code files in artifact dirs are still not promoted.
        assert_eq!(infer_artifact_kind("artifacts/helper.ts"), None);
    }

    #[test]
    fn path_rejection() {
        assert!(safe_relative_path("/proj", "../etc/passwd").is_none());
        assert!(safe_relative_path("/proj", "/abs/path").is_none());
        assert!(safe_relative_path("/proj", "valid/path.csv").is_some());
    }
}
