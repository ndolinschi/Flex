//! Temp blobs, debug log path, version, diagnostics export.

use super::prelude::*;

/// Extensions accepted by `write_temp_blob` — kept in sync with the
/// composer's paste/drop image filter (`composerAttachments.ts`'s
/// `extForMimeType`) and the file-picker's image filter (`Composer.tsx`'s
/// `handlePick`).
const TEMP_BLOB_ALLOWED_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// Hard cap on a pasted/dropped image blob, matching the size a user could
/// plausibly paste from a screenshot tool — generous enough for real
/// screenshots, small enough to keep the temp dir from filling up.
const TEMP_BLOB_MAX_BYTES: usize = 20 * 1024 * 1024;

/// Persists a pasted/dropped image blob (raw bytes from the composer's
/// clipboard/drag handler — see `composerAttachments.ts::attachImageBlob`) to
/// a uniquely-named file in the OS temp dir and returns the absolute path.
/// This is the only way to turn an in-memory clipboard blob into a
/// `PromptAttachment.path` the engine can read (`build_prompt_input` maps
/// attachments straight to `BlobSource::Path`) — there is no in-memory/base64
/// attachment path today.
///
/// `ext` is validated against an allowlist (rejects anything but
/// png/jpg/jpeg/gif/webp) and `bytes` is capped at `TEMP_BLOB_MAX_BYTES` to
/// keep this from being used to dump arbitrary/oversized files into temp.
/// Callers are responsible for cleanup — these are ordinary temp files, not
/// tracked or garbage-collected by the engine.
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

/// Absolute path of the backend's rolling debug log file (see
/// `lib.rs::init_tracing`), for the Settings Diagnostics section's "copy
/// log path" / "open logs folder" affordance.
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn debug_log_path() -> String {
    crate::debug::log_file_path()
}

/// App version baked into the desktop crate (mirrors `tauri.conf.json`).
#[tracing::instrument(level = "debug", skip_all)]
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Writes a diagnostics bundle (frontend payload + backend log tail +
/// version/OS metadata) into the app log directory. Does **not** require an
/// active session — unlike `save_text_file` / the older debug-log export —
/// so Settings → Diagnostics works on a fresh install. Returns the absolute
/// path written.
///
/// Remote crash reporting (Sentry DSN etc.) is deliberately not wired: keep
/// the payload local until a DSN + privacy review land. The frontend's
/// opt-in "crash reporting" toggle only controls whether uncaught errors
/// are retained in the in-memory crash ring included in `frontend_payload`.
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
