//! Filesystem tools and shared per-session state.
//!
//! `FsState` remembers which absolute paths were `Read` this session and the
//! file's modification time at that moment. `Write` and `Edit` consult it to
//! enforce the read-before-modify discipline and to detect files that changed
//! on disk between the model's `Read` and its mutation.

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

/// Tracks the paths read this session and their mtime at read.
///
/// One instance is shared (via `Arc`) by the `Read`, `Write`, and `Edit`
/// tools of a session.
#[derive(Default)]
pub struct FsState {
    reads: Mutex<HashMap<PathBuf, SystemTime>>,
}

impl FsState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `path` was read while its mtime was `mtime`. Also called
    /// after `Write`/`Edit` so a freshly mutated file stays editable.
    pub fn record_read(&self, path: PathBuf, mtime: SystemTime) {
        let mut map = self.reads.lock().unwrap_or_else(|p| p.into_inner());
        map.insert(path, mtime);
    }

    /// The mtime `path` had when it was last read, or `None` if it was never
    /// read this session.
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

    // --- strip_navigation_blocks tests ---
}
