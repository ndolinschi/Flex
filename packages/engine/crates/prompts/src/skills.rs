//! Skill discovery: the two-tier progressive-disclosure model.
//!
//! A skill is a directory containing `SKILL.md`: minimal YAML frontmatter
//! (`name`, `description`, optional `disable-model-invocation`) followed by a
//! markdown body. This follows the community `SKILL.md` convention (matches
//! Anthropic's public Skills spec and corpora like `mattpocock/skills`):
//! directory-per-skill, flat frontmatter, optional sibling reference files or
//! scripts the body may point at.
//!
//! Loading is two-tier, mirroring how the format is meant to be consumed:
//! - **Tier 1** (always resident): [`SkillInfo`] — just `name` + `description`
//!   — cheap enough to keep in context for every discovered skill, so the
//!   model can decide relevance without paying for the full body.
//! - **Tier 2** (on demand): [`SkillRegistry::load_body`] reads the full
//!   `SKILL.md` body (frontmatter stripped) only when a skill is actually
//!   invoked; callers (the `Skill` tool) inject it into context at that point
//!   and it stays resident for the rest of the session.
//!
//! Tier 3 (supporting files/scripts referenced from the body) needs no
//! special support here — the model reaches them with its own `Read`/`Bash`
//! tools once the body tells it to.
//!
//! The registry is deterministic, has no global state, and treats a missing
//! discovery directory as empty, matching [`crate::commands::CommandRegistry`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where a skill was discovered from, for display and override precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    /// `~/.config/agentloop/skills/*/SKILL.md`.
    User,
    /// `<project>/.agent/skills/*/SKILL.md`.
    Project,
}

/// Cheap, always-resident metadata for one discovered skill (tier 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    /// Mirrors the community convention's `disable-model-invocation: true`:
    /// when set, the skill is never offered to the model (excluded from
    /// [`SkillRegistry::model_visible`]) — reachable only by a human
    /// explicitly naming it.
    pub user_only: bool,
    pub source: SkillSource,
}

#[derive(Debug, Clone)]
struct Skill {
    info: SkillInfo,
    path: PathBuf,
}

/// Where skill directories are discovered from.
#[derive(Debug, Clone, Default)]
pub struct SkillDiscoveryConfig {
    pub user_dir: Option<PathBuf>,
    pub project_dir: Option<PathBuf>,
}

/// Errors from skill discovery or loading.
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

/// A deterministic registry of discovered skills.
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: BTreeMap<String, Skill>,
}

impl SkillRegistry {
    /// Discover from user then project directories; project skills override
    /// user skills (and user overrides nothing, since there are no built-ins)
    /// when names collide, so a project can pin its own version of a skill.
    pub fn discover(config: SkillDiscoveryConfig) -> Result<Self, SkillError> {
        let mut registry = Self::default();
        if let Some(dir) = config.user_dir {
            registry.discover_dir(&dir, SkillSource::User)?;
        }
        if let Some(dir) = config.project_dir {
            registry.discover_dir(&dir, SkillSource::Project)?;
        }
        Ok(registry)
    }

    /// Tier-1 metadata for skills the MODEL may invoke (excludes
    /// `user_only` skills), in name order — feed straight into a tool
    /// description's "available skills" listing.
    pub fn model_visible(&self) -> Vec<(String, String)> {
        self.skills
            .values()
            .filter(|skill| !skill.info.user_only)
            .map(|skill| (skill.info.name.clone(), skill.info.description.clone()))
            .collect()
    }

    /// Every discovered skill (model- and user-only alike), in name order.
    pub fn infos(&self) -> Vec<&SkillInfo> {
        self.skills.values().map(|skill| &skill.info).collect()
    }

    /// Whether any skill was discovered.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Tier 2: read `name`'s full `SKILL.md` body (frontmatter stripped).
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
            let frontmatter =
                parse_frontmatter(&raw).ok_or_else(|| SkillError::Frontmatter {
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

/// Parse the `---`-delimited frontmatter block into flat `key: value` pairs.
/// Only scalar lines are supported (no nested maps/lists, no multi-line
/// scalars) — the SKILL.md convention only ever uses flat string/bool fields,
/// so a full YAML parser (and its dependency) buys nothing here.
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

/// Strip a leading frontmatter block, if present, returning just the body.
fn strip_frontmatter(raw: &str) -> &str {
    let Some(rest) = raw.strip_prefix("---") else {
        return raw.trim();
    };
    let rest = rest.strip_prefix('\n').unwrap_or(rest);
    match rest.find("\n---") {
        Some(end) => {
            // Skip the "\n---" marker, then skip to the end of that
            // closing-fence line (tolerating trailing junk like "---  ").
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
            user_dir: None,
            project_dir: Some(dir.path().to_path_buf()),
        })
        .expect("discover");

        let visible = registry.model_visible();
        assert_eq!(visible, vec![("tdd".to_owned(), "does tdd things".to_owned())]);
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
    fn project_skills_override_user_skills_by_name() {
        let user_dir = tempdir().expect("tempdir");
        let project_dir = tempdir().expect("tempdir");
        write_skill(user_dir.path(), "review", "", "user version");
        write_skill(project_dir.path(), "review", "", "project version");

        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
            user_dir: Some(user_dir.path().to_path_buf()),
            project_dir: Some(project_dir.path().to_path_buf()),
        })
        .expect("discover");

        assert_eq!(registry.load_body("review").expect("body"), "project version");
    }

    #[test]
    fn missing_discovery_dirs_are_empty() {
        let registry = SkillRegistry::discover(SkillDiscoveryConfig {
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
        fs::write(skill_dir.join("SKILL.md"), "---\nname: broken\n---\nno description").unwrap();

        let err = SkillRegistry::discover(SkillDiscoveryConfig {
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
            user_dir: None,
            project_dir: Some(dir.path().to_path_buf()),
        })
        .expect("discover");
        assert_eq!(registry.load_body("my-skill").expect("body"), "body");
    }
}
