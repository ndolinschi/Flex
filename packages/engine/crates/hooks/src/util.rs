//! Shared helpers for the post-edit hooks: which tools count as edits, the
//! edited file, extension matching, the `$PATH` availability gate, and argv
//! substitution.

use std::path::{Path, PathBuf};

/// The tools that write file content; only these trigger post-edit hooks.
pub(crate) fn is_edit_tool(name: &str) -> bool {
    matches!(name, "Write" | "Edit")
}

/// The absolute `file_path` a `Write`/`Edit` call targeted. The fs tools
/// require absolute paths (`require_absolute` in `agentloop-tools`), so no cwd
/// resolution is needed here.
pub(crate) fn edited_file(input: &serde_json::Value) -> Option<&str> {
    input
        .get("file_path")
        .and_then(|v| v.as_str())
        .or_else(|| input.get("path").and_then(|v| v.as_str()))
}

/// Lowercased file extension without the dot, e.g. `"rs"`.
pub(crate) fn extension_of(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

/// The availability gate: whether `program` resolves to an existing file, either
/// as an explicit path or on `$PATH`. An unresolved program means the hook
/// silently skips (the correctness step only runs when the tool is available).
pub(crate) fn program_on_path(program: &str) -> bool {
    let p = Path::new(program);
    if p.is_absolute() || p.components().count() > 1 {
        return p.is_file();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

/// Replace the literal token `$FILE` in each argv element with `file`.
pub(crate) fn substitute_file(args: &[String], file: &str) -> Vec<String> {
    args.iter().map(|arg| arg.replace("$FILE", file)).collect()
}

/// The directory a post-edit command should run in: the edited file's parent.
pub(crate) fn parent_dir(file: &str) -> Option<PathBuf> {
    Path::new(file).parent().map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_write_and_edit_are_edit_tools() {
        assert!(is_edit_tool("Write"));
        assert!(is_edit_tool("Edit"));
        assert!(!is_edit_tool("Read"));
        assert!(!is_edit_tool("Bash"));
    }

    #[test]
    fn edited_file_prefers_file_path_then_path() {
        let a = serde_json::json!({"file_path": "/a.rs"});
        assert_eq!(edited_file(&a), Some("/a.rs"));
        let b = serde_json::json!({"path": "/b.rs"});
        assert_eq!(edited_file(&b), Some("/b.rs"));
        let c = serde_json::json!({"other": 1});
        assert_eq!(edited_file(&c), None);
    }

    #[test]
    fn extension_is_lowercased_without_dot() {
        assert_eq!(extension_of("/x/main.RS").as_deref(), Some("rs"));
        assert_eq!(extension_of("/x/Makefile"), None);
    }

    #[test]
    fn file_substitution_replaces_all_tokens() {
        let args = vec![
            "fmt".to_owned(),
            "$FILE".to_owned(),
            "--stdin=$FILE".to_owned(),
        ];
        assert_eq!(
            substitute_file(&args, "/w/a.rs"),
            vec!["fmt", "/w/a.rs", "--stdin=/w/a.rs"]
        );
    }

    #[test]
    fn missing_program_is_not_on_path() {
        assert!(!program_on_path("definitely-not-a-real-binary-xyz-123"));
        // An explicit non-existent path is likewise unavailable.
        assert!(!program_on_path("/no/such/tool"));
    }
}
