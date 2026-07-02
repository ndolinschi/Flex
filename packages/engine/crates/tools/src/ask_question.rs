//! `AskQuestion`: pause a turn for structured user input.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::{AgentEvent, Answer, Question, QuestionId, ToolOutput, ToolResultBlock};
use agentloop_core::{
    PendingMap, PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError,
};

use crate::fs::schema_of;

const DEFAULT_TIMEOUT_MS: u64 = 300_000;
const MAX_TIMEOUT_MS: u64 = 900_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AskQuestionInput {
    /// Questions to present to the user.
    questions: Vec<Question>,
    /// Optional timeout in milliseconds. Defaults to 300000, capped at 900000.
    timeout_ms: Option<u64>,
}

/// Emits `QuestionRequested`, waits for `respond_question`, then emits
/// `QuestionResolved`.
pub struct AskQuestionTool {
    pending: Arc<PendingMap<QuestionId, Vec<Answer>>>,
}

impl AskQuestionTool {
    pub fn new(pending: Arc<PendingMap<QuestionId, Vec<Answer>>>) -> Self {
        Self { pending }
    }
}

#[async_trait]
impl Tool for AskQuestionTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "AskUserQuestion".to_owned(),
            description: "Ask the user one or more structured questions and wait for answers. \
                          Use only when progress depends on information the model cannot infer. \
                          Each question should have a short `header`, clear `question`, optional \
                          `options`, `multi_select` when multiple options may be selected, and \
                          `allow_custom` (default true) when a typed answer is acceptable. \
                          The tool returns the user's answers and records request/resolution \
                          events."
                .to_owned(),
            input_schema: schema_of::<AskQuestionInput>(),
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
        let input: AskQuestionInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `AskUserQuestion` must be {{\"questions\": [{{\"header\": \
                 \"...\", \"question\": \"...\", \"options\": [], \"multi_select\": \
                 false}}], \"timeout_ms\": <optional milliseconds>}}: {err}."
            ))
        })?;
        if input.questions.is_empty() {
            return Err(ToolError::InvalidInput(
                "`questions` cannot be empty. Ask at least one clear question.".to_owned(),
            ));
        }
        for question in &input.questions {
            if question.header.trim().is_empty() || question.question.trim().is_empty() {
                return Err(ToolError::InvalidInput(
                    "Every question needs non-empty `header` and `question` fields.".to_owned(),
                ));
            }
        }

        let id = QuestionId::generate();
        let timeout_ms = input
            .timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);
        ctx.events.emit(AgentEvent::QuestionRequested {
            id: id.clone(),
            questions: input.questions,
        });

        let wait = self
            .pending
            .wait(id.clone(), Duration::from_millis(timeout_ms));
        let answers = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            answers = wait => answers.ok_or_else(|| {
                ToolError::Execution(format!(
                    "No answer received for AskUserQuestion `{id}` before the {timeout_ms}ms timeout. \
                     Continue with available information or ask a narrower question."
                ))
            })?,
        };

        ctx.events.emit(AgentEvent::QuestionResolved {
            id,
            answers: answers.clone(),
        });
        Ok(ToolOutput {
            content: vec![
                ToolResultBlock::markdown(format!("Received {} answer set(s).", answers.len())),
                ToolResultBlock::Json {
                    value: serde_json::json!({ "answers": answers }),
                },
            ],
            is_error: false,
            structured: Some(serde_json::json!({ "answers": answers })),
        })
    }
}
