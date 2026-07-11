//! Bundled skills: engine-curated `SKILL.md`s installed into the user skill
//! directory, embedded at compile time.
//!
//! Mirrors how the built-in system-prompt parts ship (`assembler::BUILT_IN_PARTS`,
//! `include_str!` from `packages/engine/prompts/`): the `prompts/` tree is data
//! that lives next to this crate at dev time but has no guaranteed filesystem
//! path once the binary is installed, so bundled skill bodies are compiled in
//! rather than copied from a source-tree path.
//!
//! Installation is a one-way, non-destructive seed: [`install_bundled_skills`]
//! writes each bundled skill's directory under a target skills root only if no
//! directory of that name already exists there, so a user's own customization
//! of a same-named skill is never overwritten. This mirrors the precedence
//! rule in [`crate::skills::SkillRegistry::discover`] (project > user >
//! learned) — here applied at install time instead of discovery time, since a
//! bundled skill has no dedicated [`crate::skills::SkillSource`] variant of its
//! own; once installed, it is discovered as an ordinary user-dir skill.

use std::fs;
use std::path::Path;

/// One bundled skill: its directory name and its `SKILL.md` contents.
struct BundledSkill {
    /// Directory name under the skills root, e.g. `debugging-and-error-recovery`.
    dir_name: &'static str,
    /// Full `SKILL.md` contents (frontmatter + body), embedded at compile time.
    skill_md: &'static str,
}

/// The engine-curated skill bundle, embedded at compile time from
/// `packages/engine/prompts/skills-bundled/`.
///
/// Ordered alphabetically by directory name; order has no behavioral effect
/// (install is independent per-directory), it just keeps diffs stable.
const BUNDLED_SKILLS: [BundledSkill; 6] = [
    BundledSkill {
        dir_name: "code-simplification",
        skill_md: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/skills-bundled/code-simplification/SKILL.md"
        )),
    },
    BundledSkill {
        dir_name: "debugging-and-error-recovery",
        skill_md: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/skills-bundled/debugging-and-error-recovery/SKILL.md"
        )),
    },
    BundledSkill {
        dir_name: "doubt-driven-development",
        skill_md: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/skills-bundled/doubt-driven-development/SKILL.md"
        )),
    },
    BundledSkill {
        dir_name: "performance-optimization",
        skill_md: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/skills-bundled/performance-optimization/SKILL.md"
        )),
    },
    BundledSkill {
        dir_name: "security-and-hardening",
        skill_md: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/skills-bundled/security-and-hardening/SKILL.md"
        )),
    },
    BundledSkill {
        dir_name: "using-flex-skills",
        skill_md: include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../prompts/skills-bundled/using-flex-skills/SKILL.md"
        )),
    },
];

/// Errors from installing the bundled skill set.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BundledSkillError {
    /// Could not create a bundled skill's directory under the skills root.
    #[error("cannot create bundled skill directory `{}`: {source}", path.display())]
    CreateDir {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Could not write a bundled skill's `SKILL.md`.
    #[error("cannot write bundled skill file `{}`: {source}", path.display())]
    WriteFile {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Seeds `skills_root` with any bundled skill whose directory doesn't already
/// exist there, then returns the names actually installed (empty if the root
/// already has all of them, or already existed with every name taken).
///
/// User customization always wins: a pre-existing directory of the same name
/// — whether it's the user's own fork of a bundled skill or something
/// unrelated that happens to share the name — is never touched. Call this
/// before [`crate::skills::SkillRegistry::discover`] so a freshly installed
/// bundled skill is visible in the same run.
pub fn install_bundled_skills(skills_root: &Path) -> Result<Vec<&'static str>, BundledSkillError> {
    let mut installed = Vec::new();
    for skill in &BUNDLED_SKILLS {
        let dir = skills_root.join(skill.dir_name);
        if dir.exists() {
            continue;
        }
        fs::create_dir_all(&dir).map_err(|source| BundledSkillError::CreateDir {
            path: dir.clone(),
            source,
        })?;
        let skill_md_path = dir.join("SKILL.md");
        fs::write(&skill_md_path, skill.skill_md).map_err(|source| {
            BundledSkillError::WriteFile {
                path: skill_md_path.clone(),
                source,
            }
        })?;
        installed.push(skill.dir_name);
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn installs_every_bundled_skill_into_an_empty_root() {
        let dir = tempdir().expect("tempdir");
        let installed = install_bundled_skills(dir.path()).expect("install");
        assert_eq!(installed.len(), BUNDLED_SKILLS.len());
        for skill in &BUNDLED_SKILLS {
            let skill_md = dir.path().join(skill.dir_name).join("SKILL.md");
            assert!(
                skill_md.is_file(),
                "{} missing after install",
                skill.dir_name
            );
            let contents = fs::read_to_string(&skill_md).expect("read SKILL.md");
            assert!(contents.starts_with("---\nname:"));
        }
    }

    #[test]
    fn never_overwrites_an_existing_directory_of_the_same_name() {
        let dir = tempdir().expect("tempdir");
        let existing = dir.path().join("code-simplification");
        fs::create_dir_all(&existing).expect("mkdir");
        fs::write(
            existing.join("SKILL.md"),
            "---\nname: custom\ndescription: mine\n---\nuser version",
        )
        .expect("write custom SKILL.md");

        let installed = install_bundled_skills(dir.path()).expect("install");
        assert!(
            !installed.contains(&"code-simplification"),
            "must not report overwriting an existing directory"
        );

        let contents = fs::read_to_string(existing.join("SKILL.md")).expect("read");
        assert_eq!(
            contents,
            "---\nname: custom\ndescription: mine\n---\nuser version"
        );
    }

    #[test]
    fn second_install_is_a_no_op_once_everything_exists() {
        let dir = tempdir().expect("tempdir");
        let first = install_bundled_skills(dir.path()).expect("first install");
        assert_eq!(first.len(), BUNDLED_SKILLS.len());
        let second = install_bundled_skills(dir.path()).expect("second install");
        assert!(second.is_empty(), "nothing left to install the second time");
    }

    #[test]
    fn every_bundled_skill_has_a_name_and_description_in_frontmatter() {
        for skill in &BUNDLED_SKILLS {
            assert!(
                skill.skill_md.contains("name:"),
                "{} missing name in frontmatter",
                skill.dir_name
            );
            assert!(
                skill.skill_md.contains("description:"),
                "{} missing description in frontmatter",
                skill.dir_name
            );
        }
    }
}
