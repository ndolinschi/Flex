//! Debug-mode flag + rolling log file location for the desktop shell.
//!
//! The frontend's Settings "Debug logging" toggle (see `src/lib/debug/log.ts`
//! and `src/pages/settings/DiagnosticsSection.tsx`) is the single app-wide
//! switch, persisted via the existing `tauri-plugin-store` UI store
//! (`ui.json`, key `"state".debugLoggingEnabled`). This module reads that
//! same file directly at startup — before any Tauri plugin has initialized —
//! so `lib.rs::init_tracing` can pick the right log level without a second,
//! easily-drifting sidecar flag.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

/// Matches `UI_STORE_FILE` in `src/stores/persist.ts` — the same
/// `tauri-plugin-store` JSON file the frontend's `debugLoggingEnabled` flag
/// lives in (`{"state": {"debugLoggingEnabled": true, ...}}`).
const UI_STORE_FILE: &str = "ui.json";

#[derive(Debug, Default, Deserialize)]
struct UiStoreState {
    #[serde(default)]
    debug_logging_enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct UiStoreFile {
    #[serde(default, rename = "state")]
    state: UiStoreState,
}

/// Reads the persisted `debugLoggingEnabled` flag straight out of the
/// frontend's own UI store file (`<app_data_dir>/ui.json`). Falls back to
/// `false` on any error (missing file on first launch, malformed JSON,
/// mid-write race with the store's `autoSave`) — debug mode defaults OFF,
/// so any failure here fails safe (quieter logs, not louder).
pub fn is_debug_mode_enabled(app_data_dir: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(app_data_dir.join(UI_STORE_FILE)) else {
        return false;
    };
    serde_json::from_str::<UiStoreFile>(&raw)
        .map(|f| f.state.debug_logging_enabled)
        .unwrap_or(false)
}

/// Resolved path of the rolling backend log file currently being written to
/// — set once by `init_tracing` (see `lib.rs`) since `tracing_appender`'s
/// rolling writer doesn't expose its own path. Read by the
/// `debug_log_path` command for the Settings "copy log path" affordance.
static LOG_FILE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn set_log_file_path(path: PathBuf) {
    let _ = LOG_FILE_PATH.set(path);
}

/// Best-effort log file path for the frontend. Returns a human-readable
/// placeholder (rather than an error) if logging failed to initialize —
/// this only backs a "copy path" button, not a functional dependency.
pub fn log_file_path() -> String {
    LOG_FILE_PATH
        .get()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(log file unavailable)".to_string())
}
