//! Project-instruction preflight: scan a working directory for agent/AI
//! coding-tool configuration files and format them into a system-prompt
//! section.
//!
//! This is a pure Rust, synchronous I/O scan — no LLM calls, no async.
//! Files are read in priority order; the total character budget is enforced
//! globally across all loaded files, with an explicit truncation marker
//! when exceeded. Missing files and unreadable files are silently skipped so
//! a broken or absent project config never breaks session startup.
//!
//! Skills directories (`.agent/skills/`, `.claude/skills/`,
//! `.github/skills/`) are noted by *path only* — their full bodies are not
//! loaded here; that is the job of [`crate::SkillRegistry`].

use std::fs;
use std::path::{Path, PathBuf};

/// Default total character budget for all project instructions combined.
///
/// Deliberately capped: this section rides every turn, so keeping it
/// concise matters more than completeness.
pub const DEFAULT_PROJECT_INSTRUCTIONS_BUDGET_CHARS: usize = 12_000;

/// A single instruction file that was successfully loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    /// Path relative to `cwd` (e.g. `"AGENTS.md"`, `".cursor/rules/style.md"`).
    pub relative_path: String,
    /// UTF-8 content, possibly truncated if the budget was exhausted.
    pub content: String,
    /// Original byte count (before any truncation).
    pub bytes: usize,
}

/// All project-instruction content discovered under a working directory.
#[derive(Debug, Clone, Default)]
pub struct ProjectInstructions {
    /// Loaded files in priority order.
    pub files: Vec<LoadedFile>,
    /// Skill directories that exist (bodies are NOT loaded here).
    pub skill_dirs: Vec<PathBuf>,
}

/// Priority-ordered single-file candidates (relative to `cwd`).
static CANDIDATE_FILES: &[&str] = &[
    "AGENTS.md",
    "agents.md",
    "CLAUDE.md",
    "claude.md",
    ".github/copilot-instructions.md",
    ".cursorrules",
    ".windsurfrules",
    "GEMINI.md",
    "delegation.md",
    ".agent/delegation.md",
];

/// Skill directories whose existence is noted (bodies not loaded here).
static SKILL_DIR_CANDIDATES: &[&str] = &[".agent/skills", ".claude/skills", ".github/skills"];

/// Maximum number of `.cursor/rules/` files loaded per session.
const MAX_CURSOR_RULES_FILES: usize = 20;

/// Load project instructions from `cwd` up to `budget_chars` total characters.
///
/// Pass `0` for `budget_chars` to use [`DEFAULT_PROJECT_INSTRUCTIONS_BUDGET_CHARS`].
/// The function never panics and never propagates I/O errors.
pub fn load_project_instructions(cwd: &Path, budget_chars: usize) -> ProjectInstructions {
    let budget = if budget_chars == 0 {
        DEFAULT_PROJECT_INSTRUCTIONS_BUDGET_CHARS
    } else {
        budget_chars
    };

    let mut result = ProjectInstructions::default();
    let mut used = 0usize;

    for rel in CANDIDATE_FILES {
        if used >= budget {
            break;
        }
        try_load_file(cwd, rel, &mut result.files, &mut used, budget);
    }

    // .cursor/rules/*.{md,mdc} — sorted, each prefixed with a filename header.
    if used < budget {
        load_cursor_rules(
            &cwd.join(".cursor").join("rules"),
            cwd,
            &mut result.files,
            &mut used,
            budget,
        );
    }

    for rel in SKILL_DIR_CANDIDATES {
        let dir = cwd.join(rel);
        if dir.is_dir() {
            result.skill_dirs.push(dir);
        }
    }

    result
}

/// Attempt to load a single file at `cwd/rel_path`, charge the budget, and
/// append a [`LoadedFile`] when the file is readable and non-empty.
fn try_load_file(
    cwd: &Path,
    rel_path: &str,
    files: &mut Vec<LoadedFile>,
    used: &mut usize,
    budget: usize,
) {
    let path = cwd.join(rel_path);
    if path.is_symlink() && is_escaping_symlink(cwd, &path) {
        return;
    }
    let Ok(raw) = fs::read_to_string(&path) else {
        return;
    };
    let text = raw.trim();
    if text.is_empty() {
        return;
    }
    let bytes = text.len();
    let remaining = budget.saturating_sub(*used);
    let (content, charged) = if bytes > remaining {
        let truncated: String = text.chars().take(remaining).collect();
        let with_marker = format!("{truncated}\n[truncated — budget exhausted]");
        (with_marker, remaining)
    } else {
        (text.to_owned(), bytes)
    };
    *used += charged;
    files.push(LoadedFile {
        relative_path: rel_path.to_owned(),
        content,
        bytes,
    });
}

/// Load `.cursor/rules/*.{md,mdc}` files in name-sorted order, each prefixed
/// with a `### <filename>` header.
fn load_cursor_rules(
    rules_dir: &Path,
    cwd: &Path,
    files: &mut Vec<LoadedFile>,
    used: &mut usize,
    budget: usize,
) {
    if !rules_dir.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(rules_dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext == "md" || ext == "mdc")
        })
        .collect();
    paths.sort();

    for path in paths.into_iter().take(MAX_CURSOR_RULES_FILES) {
        if *used >= budget {
            break;
        }
        if path.is_symlink() && is_escaping_symlink(cwd, &path) {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let body = raw.trim();
        if body.is_empty() {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("rule")
            .to_owned();
        let full = format!("### {filename}\n{body}");
        let bytes = full.len();
        let remaining = budget.saturating_sub(*used);
        let (content, charged) = if bytes > remaining {
            let truncated: String = full.chars().take(remaining).collect();
            (
                format!("{truncated}\n[truncated — budget exhausted]"),
                remaining,
            )
        } else {
            (full, bytes)
        };
        *used += charged;
        let rel = path
            .strip_prefix(cwd)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| filename);
        files.push(LoadedFile {
            relative_path: rel,
            content,
            bytes,
        });
    }
}

/// Best-effort check: does `link` resolve to a target outside `cwd`?
/// Returns `false` (= don't skip) when either path cannot be resolved.
fn is_escaping_symlink(cwd: &Path, link: &Path) -> bool {
    let Ok(canonical_cwd) = cwd.canonicalize() else {
        return false;
    };
    let Ok(canonical_target) = link.canonicalize() else {
        return false;
    };
    !canonical_target.starts_with(&canonical_cwd)
}

/// Format loaded project instructions into a single markdown section, or
/// return `None` when nothing was found.
///
/// Each file appears in a fenced code block with its filename as the fence
/// language tag. Skill directories are listed at the end as a bullet note.
pub fn format_project_instructions_section(loaded: &ProjectInstructions) -> Option<String> {
    if loaded.files.is_empty() {
        return None;
    }

    let mut section = String::from(
        "# Project instructions (preflight)\n\
         Discovered in the project working directory (Rust scan — not model-fetched). \
         Follow them in addition to your base instructions. Later files in this list \
         refine earlier ones when they conflict.\n",
    );

    for file in &loaded.files {
        let fence_tag = Path::new(&file.relative_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&file.relative_path);
        section.push_str(&format!(
            "\n## `{}`\n\n```{}\n{}\n```\n",
            file.relative_path, fence_tag, file.content
        ));
    }

    if !loaded.skill_dirs.is_empty() {
        section.push_str(
            "\n## Available skill directories\n\
             Use the Skill tool to load and invoke a skill by name:\n",
        );
        for dir in &loaded.skill_dirs {
            section.push_str(&format!("- `{}`\n", dir.display()));
        }
    }

    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn loads_agents_md_before_claude_md() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("AGENTS.md"), "# Agent rules\nBe concise.").unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "# Claude rules").unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        assert_eq!(loaded.files.len(), 2);
        assert_eq!(loaded.files[0].relative_path, "AGENTS.md");
        assert_eq!(loaded.files[1].relative_path, "CLAUDE.md");
    }

    #[test]
    fn missing_files_are_silently_skipped() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("GEMINI.md"), "gemini rules").unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].relative_path, "GEMINI.md");
    }

    #[test]
    fn empty_files_are_silently_skipped() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("AGENTS.md"), "   \n  ").unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "content").unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].relative_path, "CLAUDE.md");
    }

    #[test]
    fn budget_truncates_first_file_and_stops_loading_second() {
        let dir = tempdir().unwrap();
        let long_content = "x".repeat(200);
        fs::write(dir.path().join("AGENTS.md"), &long_content).unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "should not be loaded").unwrap();

        let loaded = load_project_instructions(dir.path(), 50);
        assert!(!loaded.files.is_empty());
        assert!(loaded.files[0].content.contains("[truncated"));
        assert!(loaded.files.iter().all(|f| f.relative_path != "CLAUDE.md"));
    }

    #[test]
    fn cursor_rules_loaded_in_name_order_with_headers() {
        let dir = tempdir().unwrap();
        let rules_dir = dir.path().join(".cursor").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("style.md"), "Use 4-space indent.").unwrap();
        fs::write(rules_dir.join("naming.mdc"), "PascalCase for types.").unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        assert_eq!(loaded.files.len(), 2);
        // naming.mdc < style.md alphabetically.
        assert!(loaded.files[0].relative_path.contains("naming.mdc"));
        assert!(loaded.files[1].relative_path.contains("style.md"));
        assert!(loaded.files[0].content.contains("### naming.mdc"));
        assert!(loaded.files[0].content.contains("PascalCase for types."));
    }

    #[test]
    fn non_md_mdc_files_in_cursor_rules_are_ignored() {
        let dir = tempdir().unwrap();
        let rules_dir = dir.path().join(".cursor").join("rules");
        fs::create_dir_all(&rules_dir).unwrap();
        fs::write(rules_dir.join("style.md"), "md rule").unwrap();
        fs::write(rules_dir.join("ignored.json"), "not a rule").unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        assert_eq!(loaded.files.len(), 1);
        assert!(loaded.files[0].relative_path.contains("style.md"));
    }

    #[test]
    fn skill_dirs_are_discovered_when_they_exist() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".agent").join("skills")).unwrap();
        fs::create_dir_all(dir.path().join(".claude").join("skills")).unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        assert_eq!(loaded.skill_dirs.len(), 2);
    }

    #[test]
    fn format_returns_none_when_no_files_loaded() {
        let instructions = ProjectInstructions::default();
        assert!(format_project_instructions_section(&instructions).is_none());
    }

    #[test]
    fn format_includes_all_files_fenced() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("AGENTS.md"), "Be helpful.").unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "Be concise.").unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        let section = format_project_instructions_section(&loaded).unwrap();
        assert!(section.contains("Project instructions (preflight)"));
        assert!(section.contains("AGENTS.md"));
        assert!(section.contains("Be helpful."));
        assert!(section.contains("CLAUDE.md"));
        assert!(section.contains("Be concise."));
    }

    #[test]
    fn format_includes_skills_note_when_dirs_present() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("AGENTS.md"), "rules").unwrap();
        fs::create_dir_all(dir.path().join(".agent").join("skills")).unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        let section = format_project_instructions_section(&loaded).unwrap();
        assert!(section.contains("skill"));
    }

    #[test]
    fn no_panic_on_nonexistent_cwd() {
        let loaded = load_project_instructions(Path::new("/definitely/does/not/exist"), 0);
        assert!(loaded.files.is_empty());
        assert!(loaded.skill_dirs.is_empty());
    }

    #[test]
    fn loaded_file_bytes_reflects_original_length() {
        let dir = tempdir().unwrap();
        let original = "y".repeat(100);
        fs::write(dir.path().join("AGENTS.md"), &original).unwrap();

        let loaded = load_project_instructions(dir.path(), 20);
        assert_eq!(loaded.files[0].bytes, 100);
        assert!(loaded.files[0].content.len() < 100);
    }

    #[test]
    fn cursorrules_file_loaded() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".cursorrules"), "always use types").unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        assert!(
            loaded
                .files
                .iter()
                .any(|f| f.relative_path == ".cursorrules")
        );
    }

    #[test]
    fn delegation_md_loaded_from_both_locations() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("delegation.md"), "main delegation").unwrap();
        fs::create_dir_all(dir.path().join(".agent")).unwrap();
        fs::write(
            dir.path().join(".agent").join("delegation.md"),
            "agent delegation",
        )
        .unwrap();

        let loaded = load_project_instructions(dir.path(), 0);
        let paths: Vec<&str> = loaded
            .files
            .iter()
            .map(|f| f.relative_path.as_str())
            .collect();
        assert!(paths.contains(&"delegation.md"));
        assert!(paths.contains(&".agent/delegation.md"));
    }
}
