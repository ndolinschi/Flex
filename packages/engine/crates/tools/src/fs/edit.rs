//! `Edit`: byte-exact string replacement in a freshly-read file.

use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use super::{FsState, check_freshness, modified_time, require_absolute, schema_of};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EditInput {
    /// Absolute path to edit.
    file_path: String,
    /// Exact text to replace. Must occur once unless `replace_all` is true.
    old_string: String,
    /// Replacement text.
    new_string: String,
    /// Replace every occurrence instead of requiring a unique match.
    replace_all: Option<bool>,
}

/// Replace exact text in a file the model has already read.
pub struct EditTool {
    state: Arc<FsState>,
}

impl EditTool {
    pub fn new(state: Arc<FsState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Tool for EditTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Edit".to_owned(),
            description: "Replace exact text in a file. The `file_path` must be absolute and the \
                          file must have been `Read` earlier in this session. By default \
                          `old_string` must match exactly once; include enough unchanged context \
                          around the edit. Set `replace_all: true` only for deliberate bulk \
                          replacements."
                .to_owned(),
            input_schema: schema_of::<EditInput>(),
            read_only: false,
            category: ToolCategory::Fs,
            needs_permission: PermissionHint::IfMutating,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: EditInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `Edit` must be {{\"file_path\": \"/absolute/path\", \
                 \"old_string\": \"...\", \"new_string\": \"...\", \
                 \"replace_all\": <optional bool>}}: {err}."
            ))
        })?;
        if input.old_string.is_empty() {
            return Err(ToolError::InvalidInput(
                "`old_string` cannot be empty. Use Write for whole-file replacement.".to_owned(),
            ));
        }

        let path = require_absolute(&input.file_path, &ctx.cwd)?;
        let metadata = tokio::fs::metadata(&path).await.map_err(|err| {
            ToolError::Execution(format!(
                "Cannot Edit `{}`: {err}. Read the file first and verify it exists.",
                path.display()
            ))
        })?;
        if !metadata.is_file() {
            return Err(ToolError::InvalidInput(format!(
                "`{}` is not a regular file. Pass a file path for Edit.",
                path.display()
            )));
        }
        check_freshness(&self.state, &path, modified_time(&metadata), "Edit")?;

        let bytes = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = tokio::fs::read(&path) => result.map_err(|err| {
                ToolError::Execution(format!("Cannot read `{}` for Edit: {err}.", path.display()))
            })?,
        };
        let content = String::from_utf8(bytes).map_err(|_| {
            ToolError::Execution(format!(
                "`{}` is not valid UTF-8. Edit currently supports text files only.",
                path.display()
            ))
        })?;

        let matches = content.matches(&input.old_string).count();
        if matches == 0 {
            return Err(ToolError::Execution(format!(
                "`old_string` was not found in `{}`. Read the file again and copy the exact text \
                 to replace.",
                path.display()
            )));
        }
        if matches > 1 && !input.replace_all.unwrap_or(false) {
            return Err(ToolError::Execution(format!(
                "`old_string` occurs {matches} times in `{}`. Add more surrounding context for a \
                 unique edit, or set `replace_all: true` for an intentional bulk replacement.",
                path.display()
            )));
        }

        let updated = if input.replace_all.unwrap_or(false) {
            content.replace(&input.old_string, &input.new_string)
        } else {
            content.replacen(&input.old_string, &input.new_string, 1)
        };

        tokio::select! {
            _ = ctx.cancel.cancelled() => Err(ToolError::Cancelled),
            result = tokio::fs::write(&path, updated.as_bytes()) => {
                result.map_err(|err| {
                    ToolError::Execution(format!("Cannot write edited `{}`: {err}.", path.display()))
                })?;
                let metadata = tokio::fs::metadata(&path).await.map_err(|err| {
                    ToolError::Execution(format!(
                        "Edited `{}` but could not stat it afterward: {err}.",
                        path.display()
                    ))
                })?;
                self.state.record_read(path.clone(), modified_time(&metadata));
                Ok(ToolOutput::text(format!(
                    "Edited `{}` ({matches} replacement{}).",
                    path.display(),
                    if matches == 1 { "" } else { "s" }
                )))
            }
        }
    }
}
