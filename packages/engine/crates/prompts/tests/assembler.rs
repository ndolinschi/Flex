use std::fs;

use agentloop_prompts::{PromptError, SystemPromptAssembler, SystemPromptConfig, Vars};
use pretty_assertions::assert_eq;

fn fixed_vars() -> Vars {
    Vars {
        cwd: "/workspace/project".to_owned(),
        date: "2026-01-01".to_owned(),
    }
}

#[test]
fn default_prompt_snapshot() {
    let assembler = SystemPromptAssembler::new(SystemPromptConfig::default());
    let prompt = assembler.assemble(&fixed_vars()).unwrap();
    insta::assert_snapshot!("default_prompt", prompt);
}

#[test]
fn default_prompt_substitutes_all_placeholders() {
    let assembler = SystemPromptAssembler::new(SystemPromptConfig::default());
    let prompt = assembler.assemble(&fixed_vars()).unwrap();
    assert!(prompt.contains("/workspace/project"));
    assert!(prompt.contains("2026-01-01"));
    assert!(!prompt.contains("{{cwd}}"));
    assert!(!prompt.contains("{{date}}"));
}

#[test]
fn override_dir_replaces_and_merges_parts() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("00-identity.md"),
        "# Custom identity\n\nOverride in {{cwd}}.\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("15-extra.md"),
        "# Extra guidance\n\nProject-specific rules.\n",
    )
    .unwrap();
    fs::write(dir.path().join("notes.txt"), "not a part").unwrap();

    let assembler = SystemPromptAssembler::new(SystemPromptConfig {
        parts_dir: Some(dir.path().to_path_buf()),
        appends: vec!["Appended instructions.".to_owned()],
    });
    let prompt = assembler.assemble(&fixed_vars()).unwrap();

    assert!(prompt.starts_with("# Custom identity"));
    assert!(prompt.contains("Override in /workspace/project."));
    assert!(!prompt.contains("precise software engineering agent"));
    let conduct = prompt.find("# Conduct").unwrap();
    let extra = prompt.find("# Extra guidance").unwrap();
    let tool_use = prompt.find("# Tool use").unwrap();
    assert!(conduct < extra && extra < tool_use);
    assert!(!prompt.contains("not a part"));
    assert!(prompt.ends_with("Appended instructions."));
}

#[test]
fn assembly_is_deterministic() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("05-alpha.md"), "alpha part\n").unwrap();
    fs::write(dir.path().join("25-beta.md"), "beta part\n").unwrap();
    let assembler = SystemPromptAssembler::new(SystemPromptConfig {
        parts_dir: Some(dir.path().to_path_buf()),
        appends: vec!["first append".to_owned(), "second append".to_owned()],
    });
    let first = assembler.assemble(&fixed_vars()).unwrap();
    let second = assembler.assemble(&fixed_vars()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn empty_parts_are_dropped_from_the_join() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("30-verification.md"), "\n  \n").unwrap();
    let assembler = SystemPromptAssembler::new(SystemPromptConfig {
        parts_dir: Some(dir.path().to_path_buf()),
        appends: vec![String::new()],
    });
    let prompt = assembler.assemble(&fixed_vars()).unwrap();
    assert!(!prompt.contains("# Verification"));
    assert!(!prompt.contains("\n\n\n"));
    assert!(!prompt.ends_with('\n'));
}

#[test]
fn missing_parts_dir_is_an_error_with_path_context() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("does-not-exist");
    let assembler = SystemPromptAssembler::new(SystemPromptConfig {
        parts_dir: Some(missing.clone()),
        appends: Vec::new(),
    });
    let err = assembler.assemble(&fixed_vars()).unwrap_err();
    match &err {
        PromptError::PartsDir { path, .. } => assert_eq!(path, &missing),
        other => panic!("expected PartsDir error, got: {other}"),
    }
    assert!(err.to_string().contains("does-not-exist"));
}
