
use super::prelude::*;

const TEMP_BLOB_ALLOWED_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

const TEMP_BLOB_MAX_BYTES: usize = 20 * 1024 * 1024;

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn write_temp_blob(bytes: Vec<u8>, ext: String) -> DesktopResult<String> {
    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    if !TEMP_BLOB_ALLOWED_EXTS.contains(&ext.as_str()) {
        return Err(DesktopError::Message(format!(
            "unsupported image extension `{ext}` (expected one of: {})",
            TEMP_BLOB_ALLOWED_EXTS.join(", ")
        )));
    }
    if bytes.is_empty() {
        return Err(DesktopError::Message("blob is empty".into()));
    }
    if bytes.len() > TEMP_BLOB_MAX_BYTES {
        return Err(DesktopError::Message(format!(
            "image is too large ({} bytes, max {})",
            bytes.len(),
            TEMP_BLOB_MAX_BYTES
        )));
    }

    let file_name = format!(
        "flex-paste-{}-{}.{ext}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let target = std::env::temp_dir().join(file_name);
    std::fs::write(&target, &bytes)
        .map_err(|e| DesktopError::Message(format!("cannot write `{}`: {e}", target.display())))?;

    Ok(target.display().to_string())
}

#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn debug_log_path() -> String {
    crate::debug::log_file_path()
}

#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tracing::instrument(level = "debug", skip_all, err)]
#[tauri::command]
pub async fn export_diagnostics_bundle(
    app: tauri::AppHandle,
    frontend_payload: String,
) -> DesktopResult<String> {
    use std::io::{Read, Seek, SeekFrom};
    use tauri::Manager;

    let version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let log_path = crate::debug::log_file_path();
    let backend_tail = {
        const MAX_TAIL: u64 = 256 * 1024;
        match std::fs::File::open(&log_path) {
            Ok(mut f) => {
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                if len > MAX_TAIL {
                    let _ = f.seek(SeekFrom::End(-(MAX_TAIL as i64)));
                }
                let mut buf = String::new();
                let _ = f.read_to_string(&mut buf);
                buf
            }
            Err(_) => "(backend log unavailable)".to_string(),
        }
    };

    let stamp = {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    };

    let body = format!(
        "# Desktop diagnostics export — {stamp}\n\
         version: {version}\n\
         os: {os}/{arch}\n\
         backend_log: {log_path}\n\
         \n\
         ## Frontend payload\n\
         {frontend_payload}\n\
         \n\
         ## Backend log (tail ≤256KiB)\n\
         {backend_tail}\n"
    );

    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|e| DesktopError::Message(format!("cannot resolve app log dir: {e}")))?;
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| DesktopError::Message(format!("cannot create log dir: {e}")))?;

    let target = log_dir.join(format!("diagnostics-{stamp}.txt"));
    std::fs::write(&target, body)
        .map_err(|e| DesktopError::Message(format!("cannot write {}: {e}", target.display())))?;

    Ok(target.display().to_string())
}
