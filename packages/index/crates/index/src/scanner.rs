//! Git-aware repo walker: enumerates indexable files with stable content
//! hashes so the rest of the pipeline can detect "unchanged" files cheaply.
//!
//! Respects `.gitignore`/`.git/info/exclude` via the `ignore` crate, skips
//! binary files (content sniff) and anything over [`MAX_FILE_BYTES`].

use std::path::{Path, PathBuf};

use ignore::overrides::OverrideBuilder;

/// Files larger than this are skipped entirely (never indexed).
pub const MAX_FILE_BYTES: u64 = 1_024 * 1_024;

/// Directory name the index store persists itself under (see
/// `tools::shared::index_dir_for`). Always excluded from scans so the store
/// never indexes its own tantivy segments/manifest as source content.
pub const INDEX_STORE_DIR_NAME: &str = ".agentloop";

/// Number of leading bytes sniffed to decide if a file looks binary.
const SNIFF_BYTES: usize = 8_192;

/// One file discovered by the scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Path relative to the scan root, used as the stable index key.
    pub rel_path: String,
    /// blake3 hex digest of the file's raw bytes at scan time.
    pub content_hash: String,
    /// File size in bytes.
    pub size: u64,
}

/// Walk `root`, returning every text file that should be indexed.
///
/// Ignore rules (`.gitignore`, `.git/info/exclude`, global gitignore) are
/// honored; hidden files are included unless ignored (mirrors `Glob`/`Grep`).
/// Binary files (detected by a null-byte sniff of the first
/// [`SNIFF_BYTES`]) and files over [`MAX_FILE_BYTES`] are skipped silently —
/// this is a best-effort scan, not a strict manifest.
pub fn scan_repo(root: &Path) -> Vec<ScannedFile> {
    let mut overrides = OverrideBuilder::new(root);
    // Best-effort: an invalid pattern here would be a programmer error, not
    // a user-facing one, so fall back to no overrides rather than panic.
    let _ = overrides.add(&format!("!{INDEX_STORE_DIR_NAME}/"));
    let overrides = overrides.build().unwrap_or_else(|_| {
        OverrideBuilder::new(root)
            .build()
            .unwrap_or_else(|_| ignore::overrides::Override::empty())
    });

    let mut out = Vec::new();
    for entry in ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        // Honor `.gitignore` even when `root` isn't itself inside a `.git`
        // repo (e.g. a test fixture, or a subdirectory handed to us
        // directly) — the default `require_git(true)` would silently skip
        // `.gitignore` rules in that case.
        .require_git(false)
        .overrides(overrides)
        .build()
    {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if looks_binary(&bytes) {
            continue;
        }
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        out.push(ScannedFile {
            path: path.to_path_buf(),
            rel_path,
            content_hash,
            size: metadata.len(),
        });
    }
    out
}

/// Cheap binary sniff: a NUL byte in the first [`SNIFF_BYTES`] almost always
/// means non-text content. Good enough for a first pass; false negatives
/// (e.g. binary files with no NUL in the prefix) just get lexically indexed
/// as garbage text, which is harmless.
fn looks_binary(bytes: &[u8]) -> bool {
    let take = bytes.len().min(SNIFF_BYTES);
    bytes[..take].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scan_respects_gitignore() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let root = dir.path();
        fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("ignored.txt"), "secret").unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("kept.txt"), "hello world").unwrap_or_else(|e| panic!("{e}"));

        let files = scan_repo(root);
        let rel_paths: Vec<_> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert!(rel_paths.contains(&"kept.txt"), "{rel_paths:?}");
        assert!(!rel_paths.contains(&"ignored.txt"), "{rel_paths:?}");
    }

    #[test]
    fn scan_skips_binary_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let root = dir.path();
        fs::write(root.join("binary.bin"), [0u8, 1, 2, 3, 0, 0]).unwrap_or_else(|e| panic!("{e}"));
        fs::write(
            root.join("huge.txt"),
            vec![b'a'; (MAX_FILE_BYTES + 1) as usize],
        )
        .unwrap_or_else(|e| panic!("{e}"));
        fs::write(root.join("text.txt"), "plain text content").unwrap_or_else(|e| panic!("{e}"));

        let files = scan_repo(root);
        let rel_paths: Vec<_> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(rel_paths, vec!["text.txt"]);
    }

    #[test]
    fn content_hash_is_stable_across_scans() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let root = dir.path();
        fs::write(root.join("a.txt"), "same content").unwrap_or_else(|e| panic!("{e}"));

        let first = scan_repo(root);
        let second = scan_repo(root);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].content_hash, second[0].content_hash);
    }

    #[test]
    fn content_hash_changes_when_file_changes() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let root = dir.path();
        let path = root.join("a.txt");
        fs::write(&path, "version one").unwrap_or_else(|e| panic!("{e}"));
        let before = scan_repo(root);

        fs::write(&path, "version two, different").unwrap_or_else(|e| panic!("{e}"));
        let after = scan_repo(root);

        assert_ne!(before[0].content_hash, after[0].content_hash);
    }
}
