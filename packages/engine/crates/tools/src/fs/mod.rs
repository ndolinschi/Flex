mod edit;
mod helpers;
mod html;
mod read;
mod write;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

pub use edit::EditTool;
pub use read::ReadTool;
pub use write::WriteTool;

pub(crate) use helpers::{
    check_freshness, modified_time, require_absolute, resolve_search_root, schema_of,
    truncate_chars,
};
pub(crate) use html::clean_html_for_model;
pub use html::extract_page_links;

#[derive(Default)]
pub struct FsState {
    reads: Mutex<HashMap<PathBuf, SystemTime>>,
}

impl FsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_read(&self, path: PathBuf, mtime: SystemTime) {
        let mut map = self.reads.lock().unwrap_or_else(|p| p.into_inner());
        map.insert(path, mtime);
    }

    pub fn recorded_mtime(&self, path: &Path) -> Option<SystemTime> {
        let map = self.reads.lock().unwrap_or_else(|p| p.into_inner());
        map.get(path).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_core::ToolError;
    use std::path::Path;

    #[test]
    fn records_and_recalls_mtime() {
        let state = FsState::new();
        let path = PathBuf::from("/tmp/a.txt");
        assert_eq!(state.recorded_mtime(&path), None);
        let t = SystemTime::UNIX_EPOCH;
        state.record_read(path.clone(), t);
        assert_eq!(state.recorded_mtime(&path), Some(t));
    }

    #[test]
    fn require_absolute_teaches_relative_paths() {
        let err = require_absolute("src/main.rs", Path::new("/work"));
        assert!(matches!(err, Err(ToolError::InvalidInput(_))));
        let message = err.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(message.contains("absolute"), "{message}");
        assert!(message.contains("/work/src/main.rs"), "{message}");
    }
}
