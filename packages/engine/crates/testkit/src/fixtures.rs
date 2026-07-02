//! Fixture loading helpers.
//!
//! Conventions: each crate keeps recorded fixtures under
//! `tests/fixtures/<case>/...`; loaders normalize line endings so recorded
//! transcripts behave identically on every platform. Never hand-edit a
//! recorded fixture to make a test pass — re-record and explain why.

use std::path::{Path, PathBuf};

/// Directory `<crate>/tests/fixtures`, given a crate's `CARGO_MANIFEST_DIR`.
pub fn fixtures_dir(manifest_dir: &str) -> PathBuf {
    Path::new(manifest_dir).join("tests").join("fixtures")
}

/// Read a fixture file as a string with normalized (`\n`) line endings.
///
/// Panics with the failing path on error — fixtures are test inputs, and a
/// missing one is a broken test, not a runtime condition.
pub fn load_fixture(manifest_dir: &str, relative: impl AsRef<Path>) -> String {
    let path = fixtures_dir(manifest_dir).join(relative.as_ref());
    match std::fs::read_to_string(&path) {
        Ok(content) => content.replace("\r\n", "\n"),
        Err(err) => panic!("failed to read fixture {}: {err}", path.display()),
    }
}

/// Parse a fixture of newline-delimited JSON values.
pub fn load_ndjson_fixture(
    manifest_dir: &str,
    relative: impl AsRef<Path>,
) -> Vec<serde_json::Value> {
    load_fixture(manifest_dir, relative)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| match serde_json::from_str(line) {
            Ok(value) => value,
            Err(err) => panic!("invalid JSON line in fixture: {err}\nline: {line}"),
        })
        .collect()
}
