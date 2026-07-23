use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{AgentEvent, Effort, ModelRef, ToolOutput, ToolResultBlock};
use agentloop_core::{
    PermissionHint, RoutingOverride, RoutingTable, Tool, ToolCategory, ToolContext, ToolDescriptor,
    ToolError,
};

use crate::fs::schema_of;

#[derive(Debug, Clone)]
pub struct AllowedRouting {
    pub cost_mode: String,
    pub low: Vec<String>,
    pub medium: Vec<String>,
    pub high: Vec<String>,
}

impl AllowedRouting {
    pub fn allowed_models(&self) -> Vec<String> {
        match self.cost_mode.as_str() {
            "low" => self.low.clone(),
            "medium" => {
                let mut v = self.low.clone();
                v.extend(self.medium.iter().cloned());
                v
            }
            "high" => {
                if self.high.is_empty() {
                    let mut v = self.medium.clone();
                    v.extend(self.high.iter().cloned());
                    v
                } else {
                    self.high.clone()
                }
            }
            _ => {
                let mut v = self.low.clone();
                v.extend(self.medium.iter().cloned());
                v.extend(self.high.iter().cloned());
                v
            }
        }
    }

    pub fn cap_effort(&self, effort: Effort) -> Effort {
        match self.cost_mode.as_str() {
            "low" => effort.min(Effort::Medium),
            "medium" => effort.min(Effort::High),
            _ => effort,
        }
    }

    pub fn model_allowed(&self, model_id: &str) -> bool {
        let allowed = self.allowed_models();
        allowed.iter().any(|a| {
            a == model_id
                || a.split_once('/')
                    .map(|(_, m)| m == model_id)
                    .unwrap_or(false)
                || model_id
                    .split_once('/')
                    .map(|(_, m)| a == m)
                    .unwrap_or(false)
        })
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetRoutingInput {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    reason: String,
}

pub struct SetRoutingTool {
    table: Arc<RoutingTable>,
    allowed: Arc<AllowedRouting>,
}

impl SetRoutingTool {
    pub fn new(table: Arc<RoutingTable>, allowed: Arc<AllowedRouting>) -> Self {
        Self { table, allowed }
    }

    fn build_description(allowed: &AllowedRouting) -> String {
        let low_list = if allowed.low.is_empty() {
            "(none configured)".to_owned()
        } else {
            allowed.low.join(", ")
        };
        let medium_list = if allowed.medium.is_empty() {
            "(none configured)".to_owned()
        } else {
            allowed.medium.join(", ")
        };
        let high_list = if allowed.high.is_empty() {
            "(none configured)".to_owned()
        } else {
            allowed.high.join(", ")
        };
        let effort_cap = match allowed.cost_mode.as_str() {
            "low" => "Effort is capped at `medium` in low cost mode.",
            "medium" => "Effort is capped at `high` in medium cost mode.",
            _ => "No effort cap in this cost mode.",
        };
        let allowed_ids = allowed.allowed_models();
        let allowed_display = if allowed_ids.is_empty() {
            "(no models configured — call without `model` to change effort only)".to_owned()
        } else {
            allowed_ids.join(", ")
        };
        format!(
            "Change the model and/or reasoning effort for the rest of this turn. \
             Call this ONCE, early in the turn, after you have read the task and \
             decided that the default low-cost/low-effort routing is insufficient.\n\n\
             Cost mode: `{cost_mode}`. \
             Allowed models for this cost mode: {allowed_display}.\n\
             Model tiers:\n\
             - Low: {low_list}\n\
             - Medium: {medium_list}\n\
             - High: {high_list}\n\n\
             {effort_cap}\n\n\
             Effort levels: `low` (fewest tokens, fastest) → `medium` → `high` → \
             `xhigh` → `max` (exhaustive, cross-verified). \
             `reason` is recorded in the session log — one sentence \
             explaining why the task warrants escalation.",
            cost_mode = allowed.cost_mode,
        )
    }
}

fn parse_effort_wire(s: &str) -> Option<Effort> {
    match s {
        "low" => Some(Effort::Low),
        "medium" => Some(Effort::Medium),
        "high" => Some(Effort::High),
        "xhigh" => Some(Effort::XHigh),
        "max" => Some(Effort::Max),
        _ => None,
    }
}

fn effort_to_wire(e: Effort) -> &'static str {
    match e {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
        Effort::XHigh => "xhigh",
        Effort::Max => "max",
    }
}

#[async_trait]
impl Tool for SetRoutingTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "SetRouting".to_owned(),
            description: Self::build_description(&self.allowed),
            input_schema: schema_of::<SetRoutingInput>(),
            read_only: true,
            category: ToolCategory::Agent,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: SetRoutingInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "`SetRouting` input must be \
                 {{\"model\": \"provider/model\" (optional), \
                 \"effort\": \"low|medium|high|xhigh|max\" (optional), \
                 \"reason\": \"...\"}}: {err}."
            ))
        })?;

        if input.reason.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`reason` cannot be empty — it is recorded in the session log.".to_owned(),
            ));
        }
        if input.model.is_none() && input.effort.is_none() {
            return Err(ToolError::InvalidInput(
                "At least one of `model` or `effort` must be provided.".to_owned(),
            ));
        }

        let model_ref: Option<ModelRef> = if let Some(m) = input.model.as_deref() {
            let m = m.trim();
            if !self.allowed.model_allowed(m) {
                let allowed_list = self.allowed.allowed_models().join(", ");
                return Err(ToolError::InvalidInput(format!(
                    "Model `{m}` is not in the allowed list for cost mode `{}`.\n\
                     Allowed models: {allowed_list}.\n\
                     Either pick one from the list or omit `model` to keep the current model.",
                    self.allowed.cost_mode
                )));
            }
            Some(ModelRef(m.to_owned()))
        } else {
            None
        };

        let effort: Option<Effort> = if let Some(e) = input.effort.as_deref() {
            let parsed = parse_effort_wire(e.trim()).ok_or_else(|| {
                ToolError::InvalidInput(format!(
                    "`effort` must be one of \"low\", \"medium\", \"high\", \"xhigh\", \"max\"; \
                     got \"{e}\"."
                ))
            })?;
            Some(self.allowed.cap_effort(parsed))
        } else {
            None
        };

        self.table.set(
            &ctx.session_id,
            RoutingOverride {
                model: model_ref.clone(),
                effort,
            },
        );

        ctx.events.emit(AgentEvent::RoutingChanged {
            model: model_ref.as_ref().map(|m| m.0.clone()),
            effort: effort.map(effort_to_wire).map(str::to_owned),
            reason: input.reason.clone(),
        });

        let model_msg = model_ref
            .as_ref()
            .map(|m| format!("model → `{}`", m.0))
            .unwrap_or_else(|| "model unchanged".to_owned());
        let effort_msg = effort
            .map(|e| format!("effort → `{}`", effort_to_wire(e)))
            .unwrap_or_else(|| "effort unchanged".to_owned());

        Ok(ToolOutput {
            content: vec![ToolResultBlock::markdown(format!(
                "Routing updated: {model_msg}, {effort_msg}. \
                 Reason: {}. \
                 The new settings take effect on the next model call this turn.",
                input.reason
            ))],
            is_error: false,
            structured: Some(serde_json::json!({
                "model": model_ref.as_ref().map(|m| &m.0),
                "effort": effort.map(effort_to_wire),
                "reason": input.reason,
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use tokio_util::sync::CancellationToken;

    use agentloop_contracts::{Effort, SessionId, ToolCallId, TurnId};
    use agentloop_core::{EventSink, RoutingTable, Tool, ToolContext, ToolError};

    use super::{AllowedRouting, SetRoutingTool};

    fn allowed(cost_mode: &str) -> AllowedRouting {
        AllowedRouting {
            cost_mode: cost_mode.to_owned(),
            low: vec![
                "anthropic/claude-haiku-4-5".to_owned(),
                "openai/gpt-4.1-mini".to_owned(),
            ],
            medium: vec![
                "anthropic/claude-sonnet-4-5".to_owned(),
                "openai/gpt-4.1".to_owned(),
            ],
            high: vec![
                "anthropic/claude-opus-4-5".to_owned(),
                "openai/o3".to_owned(),
            ],
        }
    }

    fn make_ctx(session_id: &str) -> ToolContext {
        let (sink, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId(session_id.to_owned()),
            turn_id: TurnId::generate(),
            call_id: ToolCallId::generate(),
            cwd: PathBuf::from("/tmp"),
            cancel: CancellationToken::new(),
            events: sink,
        }
    }

    #[tokio::test]
    async fn rejects_model_outside_allowed_list() {
        let table = Arc::new(RoutingTable::new());
        let tool = SetRoutingTool::new(table, Arc::new(allowed("low")));

        let result = tool
            .run(
                make_ctx("s1"),
                serde_json::json!({
                    "model": "anthropic/claude-opus-4-5",
                    "reason": "need heavy reasoning"
                }),
            )
            .await;

        assert!(
            matches!(result, Err(ToolError::InvalidInput(_))),
            "expected InvalidInput for model outside allowed set, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn allows_model_in_allowed_list() {
        let table = Arc::new(RoutingTable::new());
        let tool = SetRoutingTool::new(table.clone(), Arc::new(allowed("auto")));
        let session = "s2";

        let output = tool
            .run(
                make_ctx(session),
                serde_json::json!({
                    "model": "anthropic/claude-opus-4-5",
                    "reason": "complex task"
                }),
            )
            .await
            .expect("should succeed");

        assert!(!output.is_error);
        let ov = table
            .get(&SessionId(session.to_owned()))
            .expect("override should be set");
        assert_eq!(ov.model.unwrap().0, "anthropic/claude-opus-4-5");
        assert!(ov.effort.is_none());
    }

    #[tokio::test]
    async fn caps_effort_for_low_cost_mode() {
        let table = Arc::new(RoutingTable::new());
        let tool = SetRoutingTool::new(table.clone(), Arc::new(allowed("low")));
        let session = "s3";

        let output = tool
            .run(
                make_ctx(session),
                serde_json::json!({
                    "effort": "max",
                    "reason": "trying max effort in low mode"
                }),
            )
            .await
            .expect("should succeed even though effort is capped");

        assert!(!output.is_error);
        let ov = table
            .get(&SessionId(session.to_owned()))
            .expect("override should be set");
        assert_eq!(ov.effort, Some(Effort::Medium));
    }

    #[tokio::test]
    async fn caps_effort_for_medium_cost_mode() {
        let table = Arc::new(RoutingTable::new());
        let tool = SetRoutingTool::new(table.clone(), Arc::new(allowed("medium")));
        let session = "s4";

        let output = tool
            .run(
                make_ctx(session),
                serde_json::json!({
                    "effort": "xhigh",
                    "reason": "trying xhigh in medium mode"
                }),
            )
            .await
            .expect("should succeed");

        assert!(!output.is_error);
        let ov = table
            .get(&SessionId(session.to_owned()))
            .expect("override should be set");
        assert_eq!(ov.effort, Some(Effort::High));
    }

    #[tokio::test]
    async fn rejects_missing_model_and_effort() {
        let table = Arc::new(RoutingTable::new());
        let tool = SetRoutingTool::new(table, Arc::new(allowed("auto")));

        let result = tool
            .run(make_ctx("s5"), serde_json::json!({ "reason": "no-op" }))
            .await;

        assert!(
            matches!(result, Err(ToolError::InvalidInput(_))),
            "expected InvalidInput when neither model nor effort given"
        );
    }

    #[tokio::test]
    async fn bare_model_suffix_is_accepted() {
        let table = Arc::new(RoutingTable::new());
        let tool = SetRoutingTool::new(table.clone(), Arc::new(allowed("auto")));
        let session = "s6";

        let output = tool
            .run(
                make_ctx(session),
                serde_json::json!({
                    "model": "claude-opus-4-5",
                    "reason": "bare suffix check"
                }),
            )
            .await
            .expect("bare suffix should be accepted");

        assert!(!output.is_error);
    }

    #[tokio::test]
    async fn clear_removes_override() {
        let table = Arc::new(RoutingTable::new());
        let sid = SessionId("s7".to_owned());
        table.set(
            &sid,
            agentloop_core::RoutingOverride {
                model: None,
                effort: Some(Effort::High),
            },
        );
        assert!(table.get(&sid).is_some());
        table.clear(&sid);
        assert!(table.get(&sid).is_none());
    }
}
