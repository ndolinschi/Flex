use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    Learned,
    User,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub user_only: bool,
    pub source: SkillSource,
}

#[derive(Debug, Clone)]
struct Skill {
    info: SkillInfo,
    path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct SkillDiscoveryConfig {
    pub learned_dir: Option<PathBuf>,
    pub user_dir: Option<PathBuf>,
    pub project_dir: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SkillError {
    #[error("cannot read skill directory `{}`: {source}", path.display())]
    Dir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "cannot read `{}`: {source}. SKILL.md must be readable UTF-8.",
        path.display()
    )]
    File {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "`{}` has no `name`/`description` in its frontmatter. \
         Expected a leading block like:\n---\nname: my-skill\ndescription: what it's for\n---",
        path.display()
    )]
    Frontmatter { path: PathBuf },
    #[error("no skill named `{0}`")]
    NotFound(String),
}

#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: BTreeMap<String, Skill>,
}

impl SkillRegistry {
    pub fn discover(config: SkillDiscoveryConfig) -> Result<Self, SkillError> {
        let mut registry = Self::default();
        if let Some(dir) = config.learned_dir {
            registry.discover_dir(&dir, SkillSource::Learned)?;
        }
        if let Some(dir) = config.user_dir {
            registry.discover_dir(&dir, SkillSource::User)?;
        }
        if let Some(dir) = config.project_dir {
            registry.discover_dir(&dir, SkillSource::Project)?;
        }
        Ok(registry)
    }

    pub fn model_visible(&self) -> Vec<(String, String)> {
        self.skills
            .values()
            .filter(|skill| !skill.info.user_only)
            .map(|skill| (skill.info.name.clone(), skill.info.description.clone()))
            .collect()
    }

    pub fn infos(&self) -> Vec<&SkillInfo> {
        self.skills.values().map(|skill| &skill.info).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn load_body(&self, name: &str) -> Result<String, SkillError> {
        let skill = self
            .skills
            .get(name)
            .ok_or_else(|| SkillError::NotFound(name.to_owned()))?;
        let raw = fs::read_to_string(&skill.path).map_err(|source| SkillError::File {
            path: skill.path.clone(),
            source,
        })?;
        Ok(strip_frontmatter(&raw).to_owned())
    }

    fn discover_dir(&mut self, dir: &Path, source: SkillSource) -> Result<(), SkillError> {
        if !dir.exists() {
            return Ok(());
        }
        let entries = fs::read_dir(dir).map_err(|source| SkillError::Dir {
            path: dir.to_path_buf(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| SkillError::Dir {
                path: dir.to_path_buf(),
                source,
            })?;
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                continue;
            }
            let skill_md = skill_dir.join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            let raw = fs::read_to_string(&skill_md).map_err(|source| SkillError::File {
                path: skill_md.clone(),
                source,
            })?;
            let frontmatter = parse_frontmatter(&raw).ok_or_else(|| SkillError::Frontmatter {
                path: skill_md.clone(),
            })?;
            let name = frontmatter
                .get("name")
                .map(String::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    skill_dir
                        .file_name()
                        .and_then(|stem| stem.to_str())
                        .map(str::to_owned)
                })
                .ok_or_else(|| SkillError::Frontmatter {
                    path: skill_md.clone(),
                })?;
            let description = frontmatter
                .get("description")
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| SkillError::Frontmatter {
                    path: skill_md.clone(),
                })?;
            let user_only = frontmatter
                .get("disable-model-invocation")
                .is_some_and(|value| value.trim() == "true");
            self.skills.insert(
                name.clone(),
                Skill {
                    info: SkillInfo {
                        name,
                        description,
                        user_only,
                        source,
                    },
                    path: skill_md,
                },
            );
        }
        Ok(())
    }
}

fn parse_frontmatter(raw: &str) -> Option<BTreeMap<String, String>> {
    let rest = raw.strip_prefix("---")?;
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    let end = rest.find("\n---")?;
    let block = &rest[..end];
    let mut map = BTreeMap::new();
    for line in block.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        map.insert(key.to_owned(), value.to_owned());
    }
    Some(map)
}

fn strip_frontmatter(raw: &str) -> &str {
    let Some(rest) = raw.strip_prefix("---") else {
        return raw.trim();
    };
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    match rest.find("\n---") {
        Some(end) => {
            let after_fence = &rest[end + "\n---".len()..];
            let body_start = after_fence
                .find('\n')
                .map(|idx| idx + 1)
                .unwrap_or(after_fence.len());
            after_fence[body_start..].trim()
        }
        None => raw.trim(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    fn write_skill(dir: &Path, name: &str, frontmatter_extra: &str, body: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).expect("mkdir");
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: does {name} things\n{frontmatter_extra}---\n{body}"
            ),
        )
        .expect("write SKILL.md");
    }

    #[test]
    fn discovers_project_skills() {
        let dir = tempdir().expect("tempdir");
        write_skill(dir.path(), "tdd", "", "Write a failing test first.");

        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: None,
            user_dir: None,
            project_dir: Some(dir.path().to_path_buf()),
        })
        .expect("discover");

        let visible = registry.model_visible();
        assert_eq!(
            visible,
            vec![("tdd".to_owned(), "does tdd things".to_owned())]
        );
        assert_eq!(
            registry.load_body("tdd").expect("body"),
            "Write a failing test first."
        );
    }

    #[test]
    fn user_only_skill_is_excluded_from_model_visible_but_still_loadable() {
        let dir = tempdir().expect("tempdir");
        write_skill(
            dir.path(),
            "handoff",
            "disable-model-invocation: true\n",
            "Summarize session state for a handoff.",
        );

        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: None,
            user_dir: None,
            project_dir: Some(dir.path().to_path_buf()),
        })
        .expect("discover");

        assert!(registry.model_visible().is_empty());
        assert_eq!(registry.infos().len(), 1);
        assert!(registry.infos()[0].user_only);
        assert!(registry.load_body("handoff").is_ok());
    }

    #[test]
    fn user_skills_override_learned_skills_by_name() {
        let learned_dir = tempdir().expect("tempdir");
        let user_dir = tempdir().expect("tempdir");
        write_skill(learned_dir.path(), "review", "", "learned version");
        write_skill(user_dir.path(), "review", "", "user version");

        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: Some(learned_dir.path().to_path_buf()),
            user_dir: Some(user_dir.path().to_path_buf()),
            project_dir: None,
        })
        .expect("discover");

        assert_eq!(registry.load_body("review").expect("body"), "user version");
        assert_eq!(registry.infos()[0].source, SkillSource::User);
    }

    #[test]
    fn project_skills_override_user_skills_by_name() {
        let user_dir = tempdir().expect("tempdir");
        let project_dir = tempdir().expect("tempdir");
        write_skill(user_dir.path(), "review", "", "user version");
        write_skill(project_dir.path(), "review", "", "project version");

        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: None,
            user_dir: Some(user_dir.path().to_path_buf()),
            project_dir: Some(project_dir.path().to_path_buf()),
        })
        .expect("discover");

        assert_eq!(
            registry.load_body("review").expect("body"),
            "project version"
        );
    }

    #[test]
    fn missing_discovery_dirs_are_empty() {
        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: None,
            user_dir: Some(PathBuf::from("/definitely/missing/skills")),
            project_dir: None,
        })
        .expect("missing dirs are ignored");
        assert!(registry.is_empty());
    }

    #[test]
    fn missing_frontmatter_field_errors_with_path() {
        let dir = tempdir().expect("tempdir");
        let skill_dir = dir.path().join("broken");
        fs::create_dir_all(&skill_dir).expect("mkdir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: broken\n---\nno description",
        )
        .unwrap();

        let err = SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: None,
            user_dir: None,
            project_dir: Some(dir.path().to_path_buf()),
        })
        .unwrap_err();
        assert!(matches!(err, SkillError::Frontmatter { .. }));
    }

    #[test]
    fn name_falls_back_to_directory_name() {
        let dir = tempdir().expect("tempdir");
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir_all(&skill_dir).expect("mkdir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: no explicit name\n---\nbody",
        )
        .unwrap();

        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
            learned_dir: None,
            user_dir: None,
            project_dir: Some(dir.path().to_path_buf()),
        })
        .expect("discover");
        assert_eq!(registry.load_body("my-skill").expect("body"), "body");
    }
}
