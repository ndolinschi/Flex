//! Shared filesystem path normalization for workspace `cwd` strings.
//!
//! Session paths round-trip through JSON and OS dialogs; on Windows they can
//! pick up doubled backslashes, and isolated sessions can leave a dead
//! worktree path in `cwd` while `base_cwd` still points at the real repo.

use std::path::{Path, PathBuf};

/// Collapse doubled `\` separators that appear when a Windows path was
/// JSON/shell double-escaped (`C:\\Users\\foo` as the filesystem string).
/// Preserves UNC (`\\server\share`) and extended (`\\?\…`) prefixes.
///
/// Double-escaped UNC arrives as four leading backslashes (`\\\\server\\…`);
/// the leading run is normalized to exactly `\\` before collapsing the rest.
pub fn collapse_extra_backslashes(path: &str) -> String {
    if path.starts_with(r"\\?\") {
        return path.to_owned();
    }
    let unc = path.starts_with(r"\\");
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    if unc {
        out.push('\\');
        out.push('\\');
        // Skip the whole leading `\` run (2 from real UNC, 4+ from double-escape).
        while chars.peek() == Some(&'\\') {
            let _ = chars.next();
        }
    }
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push('\\');
            while chars.peek() == Some(&'\\') {
                let _ = chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Trim quotes / `file://` prefixes from a dialog or wire-path string.
pub fn normalize_cwd_input(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    if trimmed.is_empty() {
        return String::new();
    }
    // `file:///Users/…` or `file:///C:/Users/…` (and `file://localhost/…`).
    let without_scheme = trimmed
        .strip_prefix("file://localhost")
        .or_else(|| trimmed.strip_prefix("file://"))
        .unwrap_or(trimmed);
    // On Windows, `file:///C:/…` → `/C:/…`; drop the leading slash before the drive.
    #[cfg(windows)]
    {
        let bytes = without_scheme.as_bytes();
        if bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
        {
            return without_scheme[1..].to_owned();
        }
    }
    without_scheme.to_owned()
}

fn try_dir(raw: &str) -> Option<PathBuf> {
    if raw.is_empty() {
        return None;
    }
    let norm = normalize_cwd_input(raw);
    if norm.is_empty() {
        return None;
    }
    let path = PathBuf::from(&norm);
    if path.is_dir() {
        return Some(path);
    }
    let collapsed = collapse_extra_backslashes(&norm);
    if collapsed != norm {
        let path = PathBuf::from(&collapsed);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

/// Resolve an existing directory from `primary`, then optional `fallback`
/// (typically `SessionMeta.base_cwd` when an isolated worktree is gone).
pub fn resolve_existing_dir(primary: &str, fallback: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = try_dir(primary) {
        return Some(path);
    }
    if let Some(fb) = fallback {
        if let Some(path) = try_dir(fb) {
            if Path::new(primary).as_os_str() != path.as_os_str() {
                tracing::warn!(
                    requested = %primary,
                    fallback = %path.display(),
                    "workspace cwd missing; falling back to base_cwd"
                );
            }
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collapse_extra_backslashes_collapses_doubled_seps() {
        assert_eq!(
            collapse_extra_backslashes(r"C:\\Users\\foo"),
            r"C:\Users\foo"
        );
        assert_eq!(collapse_extra_backslashes(r"C:\Users\foo"), r"C:\Users\foo");
    }

    #[test]
    fn collapse_extra_backslashes_preserves_unc_prefix() {
        assert_eq!(
            collapse_extra_backslashes(r"\\server\share\\dir"),
            r"\\server\share\dir"
        );
    }

    #[test]
    fn normalize_strips_file_url_and_quotes() {
        assert_eq!(
            normalize_cwd_input("  \"/tmp/project\"  "),
            "/tmp/project"
        );
        assert_eq!(
            normalize_cwd_input("file:///tmp/project"),
            "/tmp/project"
        );
    }

    #[test]
    fn resolve_existing_dir_prefers_primary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let resolved = resolve_existing_dir(&path, Some("/nonexistent")).unwrap();
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn resolve_existing_dir_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let resolved = resolve_existing_dir("/no/such/worktree", Some(&path)).unwrap();
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn resolve_existing_dir_none_when_missing() {
        assert!(resolve_existing_dir("/no/such/a", Some("/no/such/b")).is_none());
    }

    #[test]
    fn resolve_existing_dir_accepts_real_dir() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("x.txt"), b"hi").unwrap();
        assert!(resolve_existing_dir(&dir.path().to_string_lossy(), None).is_some());
    }
}
