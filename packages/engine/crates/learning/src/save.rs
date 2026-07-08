//! `SkillSave`: persist a distilled procedure as a learned `SKILL.md`.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

/// Skill names stay filesystem- and prompt-friendly.
const NAME_MAX: usize = 48;
/// Description cap: it sits in every request's context via the Skill tool.
const DESCRIPTION_MAX: usize = 300;
/// Body cap: a skill is a focused playbook, not a transcript dump.
const BODY_MAX: usize = 16_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SkillSaveInput {
    /// Kebab-case skill name, e.g. `migrate-sqlx-schema`. Lowercase letters,
    /// digits, and hyphens; max 48 chars.
    name: String,
    /// One sentence stating when this skill applies (max 300 chars). This is
    /// what future sessions see when deciding relevance.
    description: String,
    /// The skill body in markdown: the verified procedure as numbered steps,
    /// including pitfalls encountered and how they were resolved (max 16000
    /// chars). Write for a future agent with no memory of this session.
    body: String,
    /// Set true to replace an existing learned skill of the same name.
    #[serde(default)]
    overwrite: bool,
}

/// Writes learned skills under a dedicated directory, one
/// `<name>/SKILL.md` per skill, with `provenance: learned` frontmatter.
pub struct SkillSaveTool {
    learned_dir: PathBuf,
}

impl SkillSaveTool {
    pub fn new(learned_dir: impl Into<PathBuf>) -> Self {
        Self {
            learned_dir: learned_dir.into(),
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

pub(crate) fn schema_of<T: JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T))
        .unwrap_or_else(|_| serde_json::json!({"type": "object"}))
}

#[async_trait]
impl Tool for SkillSaveTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "SkillSave".to_owned(),
            description: "Persist a verified, reusable procedure from this session as a \
                          learned skill, so future sessions can load it by name. Use ONLY \
                          for procedures that were non-obvious, are likely to recur, and \
                          were verified to work in this session — never for facts, \
                          opinions, or unverified guesses. Save at most one skill per \
                          session. If a learned skill with the same name exists, the call \
                          fails and returns its current description; re-call with \
                          `overwrite: true` to replace it after merging anything still \
                          valuable."
                .to_owned(),
            input_schema: schema_of::<SkillSaveInput>(),
            read_only: false,
            category: ToolCategory::Fs,
            needs_permission: PermissionHint::Always,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: SkillSaveInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `SkillSave` must be {{\"name\", \"description\", \"body\", \
                 \"overwrite\"?}}: {err}."
            ))
        })?;
        if !valid_name(&input.name) {
            return Err(ToolError::InvalidInput(format!(
                "`name` must be kebab-case (lowercase letters, digits, hyphens; max \
                 {NAME_MAX} chars), got `{}`.",
                input.name
            )));
        }
        if input.description.trim().is_empty() || input.description.len() > DESCRIPTION_MAX {
            return Err(ToolError::InvalidInput(format!(
                "`description` must be a non-empty sentence of at most {DESCRIPTION_MAX} chars."
            )));
        }
        if input.body.trim().is_empty() || input.body.len() > BODY_MAX {
            return Err(ToolError::InvalidInput(format!(
                "`body` must be non-empty and at most {BODY_MAX} chars; distill the \
                 procedure, don't dump the transcript."
            )));
        }

        let skill_dir = self.learned_dir.join(&input.name);
        let skill_md = skill_dir.join("SKILL.md");
        if skill_md.exists() && !input.overwrite {
            let existing_description = std::fs::read_to_string(&skill_md)
                .ok()
                .and_then(|raw| {
                    raw.lines()
                        .find(|line| line.starts_with("description:"))
                        .map(|line| line.trim_start_matches("description:").trim().to_owned())
                })
                .unwrap_or_default();
            return Err(ToolError::Execution(format!(
                "A learned skill `{}` already exists (description: {existing_description}). \
                 Load it with the Skill tool to compare, merge anything still valuable, \
                 then re-call SkillSave with `overwrite: true`.",
                input.name
            )));
        }

        let description = input.description.trim().replace('\n', " ");
        let content = format!(
            "---\nname: {}\ndescription: {}\nprovenance: learned\n---\n{}\n",
            input.name,
            description,
            input.body.trim()
        );
        std::fs::create_dir_all(&skill_dir).map_err(|err| {
            ToolError::Execution(format!(
                "Cannot create learned-skill directory `{}`: {err}.",
                skill_dir.display()
            ))
        })?;
        std::fs::write(&skill_md, content).map_err(|err| {
            ToolError::Execution(format!("Cannot write `{}`: {err}.", skill_md.display()))
        })?;
        tracing::info!(target: "learning", skill = %input.name, path = %skill_md.display(), "skill learned");
        Ok(ToolOutput::text(format!(
            "Saved learned skill `{}` to {}. It becomes loadable via the Skill tool in \
             future sessions.",
            input.name,
            skill_md.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{SessionId, ToolCallId, TurnId};
    use agentloop_core::EventSink;
    use tokio_util::sync::CancellationToken;

    fn ctx() -> ToolContext {
        let (events, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId::from("sess-test"),
            turn_id: TurnId::from("turn-test"),
            call_id: ToolCallId::from("call-test"),
            cwd: PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        }
    }

    fn input(name: &str) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "description": "how to test skill saving",
            "body": "1. Do the thing.\n2. Verify it."
        })
    }

    #[tokio::test]
    async fn saves_a_new_skill_with_frontmatter() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tool = SkillSaveTool::new(dir.path());
        tool.run(ctx(), input("test-skill")).await.expect("run ok");
        let written =
            std::fs::read_to_string(dir.path().join("test-skill").join("SKILL.md")).expect("read");
        assert!(written.contains("name: test-skill"));
        assert!(written.contains("provenance: learned"));
        assert!(written.contains("1. Do the thing."));
    }

    #[tokio::test]
    async fn refuses_to_clobber_without_overwrite() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tool = SkillSaveTool::new(dir.path());
        tool.run(ctx(), input("test-skill")).await.expect("first");
        let err = tool.run(ctx(), input("test-skill")).await.unwrap_err();
        assert!(matches!(err, ToolError::Execution(msg) if msg.contains("overwrite")));

        let mut second = input("test-skill");
        second["overwrite"] = serde_json::json!(true);
        second["body"] = serde_json::json!("replaced body");
        tool.run(ctx(), second).await.expect("overwrite ok");
        let written =
            std::fs::read_to_string(dir.path().join("test-skill").join("SKILL.md")).expect("read");
        assert!(written.contains("replaced body"));
    }

    #[tokio::test]
    async fn rejects_bad_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tool = SkillSaveTool::new(dir.path());
        for bad in ["", "Has-Caps", "spaces here", "-lead", "trail-", "a/../b"] {
            let err = tool.run(ctx(), input(bad)).await.unwrap_err();
            assert!(matches!(err, ToolError::InvalidInput(_)), "name: {bad}");
        }
    }
}
