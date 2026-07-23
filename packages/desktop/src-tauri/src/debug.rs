
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

const UI_STORE_FILE: &str = "ui.json";

#[derive(Debug, Default, Deserialize)]
struct UiStoreState {
    #[serde(default, rename = "debugLoggingEnabled")]
    debug_logging_enabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct UiStoreFile {
    #[serde(default, rename = "state")]
    state: UiStoreState,
}

pub fn is_debug_mode_enabled(app_data_dir: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(app_data_dir.join(UI_STORE_FILE)) else {
        return false;
    };
    serde_json::from_str::<UiStoreFile>(&raw)
        .map(|f| f.state.debug_logging_enabled)
        .unwrap_or(false)
}

static LOG_FILE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn set_log_file_path(path: PathBuf) {
    let _ = LOG_FILE_PATH.set(path);
}

pub fn log_file_path() -> String {
    LOG_FILE_PATH
        .get()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(log file unavailable)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("desktop-debug-{label}-{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn reads_camel_case_debug_logging_enabled_from_ui_json() {
        let dir = scratch_dir("on");
        fs::write(
            dir.join(UI_STORE_FILE),
            r#"{"state":{"debugLoggingEnabled":true,"crashReportingEnabled":false}}"#,
        )
        .expect("write ui.json");
        assert!(is_debug_mode_enabled(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_false_flag_defaults_off() {
        let dir = scratch_dir("off");
        assert!(!is_debug_mode_enabled(&dir));

        fs::write(
            dir.join(UI_STORE_FILE),
            r#"{"state":{"debugLoggingEnabled":false}}"#,
        )
        .expect("write ui.json");
        assert!(!is_debug_mode_enabled(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
