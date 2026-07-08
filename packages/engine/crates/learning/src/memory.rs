//! `MemoryWrite`: persist a durable note into the local memory directory.

use std::path::PathBuf;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

const NAME_MAX: usize = 48;
/// Per-file cap; memories are always resident in the prompt, so keep them tiny.
const CONTENT_MAX: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MemoryWriteMode {
    /// Replace the note (default).
    Replace,
    /// Append to the note's end.
    Append,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MemoryWriteInput {
    /// Kebab-case note name, e.g. `user-preferences`. One topic per note.
    name: String,
    /// The note content in markdown (max 4000 chars). Only durable,
    /// user-confirmed facts and preferences — never session-specific state.
    content: String,
    /// `replace` (default) or `append`.
    #[serde(default)]
    mode: Option<MemoryWriteMode>,
}

/// Writes `<name>.md` files under the local memory directory; they load into
/// every future session's system prompt.
pub struct MemoryWriteTool {
    memory_dir: PathBuf,
}

impl MemoryWriteTool {
    pub fn new(memory_dir: impl Into<PathBuf>) -> Self {
        Self {
            memory_dir: memory_dir.into(),
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

#[async_trait]
impl Tool for MemoryWriteTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "MemoryWrite".to_owned(),
            description: "Persist a durable note that loads into every future session's \
                          context (user preferences, project facts, environment quirks). \
                          Use ONLY for facts the user stated or confirmed and that stay \
                          true across sessions — never for session-specific state, \
                          guesses, or anything the repository already records. Notes are \
                          tiny and always resident: keep each under a few hundred words, \
                          one topic per `name`. `mode: append` adds to an existing note; \
                          the default replaces it."
                .to_owned(),
            input_schema: crate::save::schema_of::<MemoryWriteInput>(),
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
        let input: MemoryWriteInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `MemoryWrite` must be {{\"name\", \"content\", \"mode\"?}}: {err}."
            ))
        })?;
        if !valid_name(&input.name) {
            return Err(ToolError::InvalidInput(format!(
                "`name` must be kebab-case (lowercase letters, digits, hyphens; max \
                 {NAME_MAX} chars), got `{}`.",
                input.name
            )));
        }
        if input.content.trim().is_empty() {
            return Err(ToolError::InvalidInput(
                "`content` cannot be empty. To drop a memory, ask the user to delete the file."
                    .to_owned(),
            ));
        }

        let path = self.memory_dir.join(format!("{}.md", input.name));
        let mut content = match input.mode {
            Some(MemoryWriteMode::Append) => {
                let existing = std::fs::read_to_string(&path).unwrap_or_default();
                if existing.trim().is_empty() {
                    input.content.trim().to_owned()
                } else {
                    format!("{}\n{}", existing.trim_end(), input.content.trim())
                }
            }
            _ => input.content.trim().to_owned(),
        };
        if content.len() > CONTENT_MAX {
            if input.mode == Some(MemoryWriteMode::Append) {
                return Err(ToolError::Execution(format!(
                    "Appending would grow `{}` past {CONTENT_MAX} chars. Rewrite the note \
                     with `mode: replace`, keeping only what still matters.",
                    input.name
                )));
            }
            return Err(ToolError::InvalidInput(format!(
                "`content` exceeds {CONTENT_MAX} chars; memories are always resident — \
                 keep them tiny."
            )));
        }
        content.push('\n');

        std::fs::create_dir_all(&self.memory_dir).map_err(|err| {
            ToolError::Execution(format!(
                "Cannot create memory directory `{}`: {err}.",
                self.memory_dir.display()
            ))
        })?;
        std::fs::write(&path, content).map_err(|err| {
            ToolError::Execution(format!("Cannot write `{}`: {err}.", path.display()))
        })?;
        tracing::info!(target: "memory", note = %input.name, path = %path.display(), "memory updated");
        Ok(ToolOutput::text(format!(
            "Saved memory `{}` to {}. It loads into every future session.",
            input.name,
            path.display()
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

    #[tokio::test]
    async fn replace_then_append() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tool = MemoryWriteTool::new(dir.path());
        tool.run(
            ctx(),
            serde_json::json!({"name": "prefs", "content": "prefers vim"}),
        )
        .await
        .expect("replace");
        tool.run(
            ctx(),
            serde_json::json!({"name": "prefs", "content": "answers in Russian", "mode": "append"}),
        )
        .await
        .expect("append");
        let written = std::fs::read_to_string(dir.path().join("prefs.md")).expect("read");
        assert_eq!(written, "prefers vim\nanswers in Russian\n");
    }

    #[tokio::test]
    async fn caps_content_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tool = MemoryWriteTool::new(dir.path());
        let err = tool
            .run(
                ctx(),
                serde_json::json!({"name": "big", "content": "x".repeat(5_000)}),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }
}
