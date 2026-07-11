//! `Agent`: delegate a scoped job to a subagent.
//!
//! This crate ships only the descriptor — execution is intercepted and run by
//! the engine loop (it needs to spawn a child session, which a pure tool
//! cannot). The description is written for the orchestrator model: it teaches
//! when to fan out searchers vs workers and how to write a self-contained
//! brief.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::tool::SUBAGENT_TOOL_NAME;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use crate::fs::schema_of;

#[allow(dead_code)]
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AgentInput {
    /// Which role runs the task (see the tool description for the list).
    role: String,
    /// A 3-7 word label shown to the user, e.g. "map session event flow".
    description: String,
    /// The self-contained brief: everything the subagent needs, since it
    /// sees none of this conversation.
    prompt: String,
    /// Precisely what the subagent should return.
    #[serde(default)]
    expected_output: Option<String>,
    /// Optional model override for this subagent (e.g.
    /// "anthropic/claude-opus-4-8" or a bare model id resolvable by the
    /// registry). Defaults to the role's models or the parent's model.
    #[serde(default)]
    model: Option<String>,
}

/// The `Agent` descriptor. `run` is never reached in a correct build — the
/// loop intercepts calls by name and runs a subagent instead.
struct AgentTool {
    description: String,
}

#[async_trait]
impl Tool for AgentTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: SUBAGENT_TOOL_NAME.to_owned(),
            description: self.description.clone(),
            input_schema: schema_of::<AgentInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        _ctx: ToolContext,
        _input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Execution(
            "the Agent tool is executed by the engine loop; this build predates \
             subagent support"
                .to_owned(),
        ))
    }
}

/// Build an `Agent` tool whose description advertises the spawnable `roles`
/// as `(name, one-line summary)` pairs.
pub fn subagent_tool(roles: &[(String, String)]) -> Arc<dyn Tool> {
    let role_lines = if roles.is_empty() {
        "  (no roles configured)".to_owned()
    } else {
        roles
            .iter()
            .map(|(name, summary)| format!("- {name} — {summary}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let description = format!(
        "Delegate a well-scoped task to a subagent that runs in its own fresh session \
         with its own tools and model, and returns a single final report. Use subagents \
         to parallelize independent work and to keep your own context small: research \
         before you build, fan out searches, split independent implementation tasks.\n\n\
         Available roles:\n{role_lines}\n\n\
         The subagent starts with NO context: it does not see this conversation, your \
         plan, or other subagents' results. Write `prompt` as a self-contained brief — \
         state the goal and exact deliverable, and include every fact it needs that you \
         already know (absolute file paths, symbol names, decisions already made). Say \
         precisely what to return in `expected_output`; only the subagent's final \
         message comes back, and there is no follow-up conversation.\n\n\
         Choosing granularity: a question answerable by reading code or docs → \
         searcher(s) in parallel; implementation after the plan is clear → one worker \
         per INDEPENDENT task (tasks touching the same files are not independent). Do \
         not delegate trivial work — the handoff costs more than doing it yourself.\n\n\
         Parallelism: emit multiple Agent calls in ONE message to run them concurrently \
         (they may be served by different models). You own integrating the results and \
         the correctness of the final answer.\n\n\
         Optional `model` override: pin this specific call to a model instead of the \
         role's own chain (e.g. escalate one subagent to a stronger model for a hard \
         task). Leave it unset to use the role's configured models."
    );
    Arc::new(AgentTool { description })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_descriptor_advertises_roles_and_model_override() {
        let descriptor =
            subagent_tool(&[("searcher".to_owned(), "finds things".to_owned())]).descriptor();
        assert_eq!(descriptor.name, SUBAGENT_TOOL_NAME);
        assert!(descriptor.read_only);
        assert_eq!(descriptor.needs_permission, PermissionHint::Never);
        assert!(descriptor.description.contains("searcher"));
        assert!(descriptor.description.contains("model"));
        let schema = descriptor.input_schema.to_string();
        assert!(
            schema.contains("\"model\""),
            "input schema advertises the optional model override: {schema}"
        );
    }

    #[test]
    fn input_with_model_parses() {
        let input: AgentInput = serde_json::from_value(serde_json::json!({
            "role": "searcher",
            "description": "map event flow",
            "prompt": "find X",
            "model": "anthropic/claude-opus-4-8",
        }))
        .expect("model field parses");
        assert_eq!(input.model.as_deref(), Some("anthropic/claude-opus-4-8"));
    }

    #[test]
    fn input_without_model_defaults_to_none() {
        let input: AgentInput = serde_json::from_value(serde_json::json!({
            "role": "searcher",
            "description": "map event flow",
            "prompt": "find X",
        }))
        .expect("model is optional");
        assert_eq!(input.model, None);
    }

    #[test]
    fn deny_unknown_fields_still_rejects_typos() {
        let err = serde_json::from_value::<AgentInput>(serde_json::json!({
            "role": "searcher",
            "description": "map event flow",
            "prompt": "find X",
            "modle": "typo",
        }))
        .unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[tokio::test]
    async fn agent_run_is_never_reached_in_a_correct_build() {
        use agentloop_contracts::{SessionId, ToolCallId, TurnId};
        use agentloop_core::EventSink;
        use tokio_util::sync::CancellationToken;

        let (events, _rx) = EventSink::channel();
        let ctx = ToolContext {
            session_id: SessionId::from("sess-test"),
            turn_id: TurnId::from("turn-test"),
            call_id: ToolCallId::from("call-test"),
            cwd: std::path::PathBuf::from("."),
            cancel: CancellationToken::new(),
            events,
        };
        let err = subagent_tool(&[])
            .run(ctx, serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Execution(_)));
    }
}
