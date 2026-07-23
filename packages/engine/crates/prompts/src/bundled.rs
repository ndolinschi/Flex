use std::fs;
use std::path::Path;

struct BundledSkill {
    dir_name: &'static str,
    skill_md: &'static str,
}

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

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BundledSkillError {
    #[error("cannot create bundled skill directory `{}`: {source}", path.display())]
    CreateDir {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot write bundled skill file `{}`: {source}", path.display())]
    WriteFile {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

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
