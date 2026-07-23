use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

#[allow(dead_code)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SkillInput {
    name: String,
}

pub type SkillLoader = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

struct SkillTool {
    description: String,
    loader: SkillLoader,
}

#[async_trait]
impl Tool for SkillTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Skill".to_owned(),
            description: self.description.clone(),
            input_schema: schema_of::<SkillInput>(),
            read_only: true,
            category: ToolCategory::Other,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let name = input
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ToolError::InvalidInput("`name` must be a string".to_owned()))?;
        (self.loader)(name)
            .map(ToolOutput::text)
            .ok_or_else(|| ToolError::Execution(format!("unknown skill `{name}`")))
    }
}

pub fn skill_tool(skills: &[(String, String)], loader: SkillLoader) -> Option<Arc<dyn Tool>> {
    if skills.is_empty() {
        return None;
    }
    let mut description = String::from(
        "Load a skill's full instructions into context. Skills are focused, \
         reusable playbooks for a specific kind of task, kept out of context \
         until relevant. Call this with a skill's `name` when its description \
         matches what you're about to do — its full guidance then stays in \
         context for the rest of the session. Available skills:\n",
    );
    for (name, summary) in skills {
        description.push_str(&format!("- `{name}`: {summary}\n"));
    }
    Some(Arc::new(SkillTool {
        description,
        loader,
    }))
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
            cwd: std::path::PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        }
    }

    #[test]
    fn empty_skill_list_yields_no_tool() {
        assert!(skill_tool(&[], Arc::new(|_: &str| None)).is_none());
    }

    #[tokio::test]
    async fn known_skill_returns_its_body() {
        let tool = skill_tool(
            &[("tdd".to_owned(), "test-driven development".to_owned())],
            Arc::new(|name| (name == "tdd").then(|| "write a failing test first".to_owned())),
        )
        .expect("tool built");
        let output = tool
            .run(ctx(), serde_json::json!({"name": "tdd"}))
            .await
            .expect("run ok");
        assert_eq!(output.content.len(), 1);
        match &output.content[0] {
            agentloop_contracts::ToolResultBlock::Markdown { text } => {
                assert_eq!(text, "write a failing test first");
            }
            other => panic!("expected markdown block, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn unknown_skill_errors() {
        let tool = skill_tool(
            &[("tdd".to_owned(), "test-driven development".to_owned())],
            Arc::new(|_: &str| None),
        )
        .expect("tool built");
        let err = tool
            .run(ctx(), serde_json::json!({"name": "nope"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(message) if message.contains("nope")));
    }

    #[test]
    fn description_lists_every_skill() {
        let tool = skill_tool(
            &[
                ("tdd".to_owned(), "test-driven development".to_owned()),
                ("review".to_owned(), "code review".to_owned()),
            ],
            Arc::new(|_: &str| None),
        )
        .expect("tool built");
        let description = tool.descriptor().description;
        assert!(description.contains("`tdd`: test-driven development"));
        assert!(description.contains("`review`: code review"));
    }
}
