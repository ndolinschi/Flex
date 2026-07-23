use std::path::{Path, PathBuf};

pub fn fixtures_dir(manifest_dir: &str) -> PathBuf {
    Path::new(manifest_dir).join("tests").join("fixtures")
}

pub fn load_fixture(manifest_dir: &str, relative: impl AsRef<Path>) -> String {
    let path = fixtures_dir(manifest_dir).join(relative.as_ref());
    match std::fs::read_to_string(&path) {
        Ok(content) => content.replace("\r\n", "\n"),
        Err(err) => panic!("failed to read fixture {}: {err}", path.display()),
    }
}

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
