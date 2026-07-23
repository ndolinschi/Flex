use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;

use agentloop_contracts::ToolOutput;
use agentloop_core::{PermissionHint, Tool, ToolCategory, ToolContext, ToolDescriptor, ToolError};

use super::{FsState, modified_time, require_absolute, schema_of, truncate_chars};

const MAX_READ_CHARS: usize = 120_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadInput {
    file_path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

pub struct ReadTool {
    state: Arc<FsState>,
}

impl ReadTool {
    pub fn new(state: Arc<FsState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "Read".to_owned(),
            description: "Read a text file by absolute `file_path`. Use this before `Edit` or \
                          overwriting an existing file with `Write`; those tools verify you are \
                          editing the version you saw. Optional `offset` is a 1-based line \
                          number and `limit` caps returned lines. Returns `LINE|content` rows \
                          and explicit truncation markers."
                .to_owned(),
            input_schema: schema_of::<ReadInput>(),
            read_only: true,
            category: ToolCategory::Fs,
            needs_permission: PermissionHint::Never,
        }
    }

    async fn run(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let input: ReadInput = serde_json::from_value(input).map_err(|err| {
            ToolError::InvalidInput(format!(
                "Input for `Read` must be {{\"file_path\": \"/absolute/path\", \
                 \"offset\": <optional 1-based line>, \"limit\": <optional lines>}}: {err}."
            ))
        })?;
        let path = require_absolute(&input.file_path, &ctx.cwd)?;

        let metadata = tokio::fs::metadata(&path).await.map_err(|err| {
            ToolError::Execution(format!(
                "Cannot Read `{}`: {err}. Check that the file exists and is readable.",
                path.display()
            ))
        })?;
        if !metadata.is_file() {
            return Err(ToolError::InvalidInput(format!(
                "`{}` is not a regular file. Pass a file path, not a directory.",
                path.display()
            )));
        }

        let bytes = tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
            result = tokio::fs::read(&path) => result.map_err(|err| {
                ToolError::Execution(format!("Cannot Read `{}`: {err}.", path.display()))
            })?,
        };
        self.state
            .record_read(path.clone(), modified_time(&metadata));

        let raw = String::from_utf8_lossy(&bytes);
        let start = input.offset.unwrap_or(1);
        if start == 0 {
            return Err(ToolError::InvalidInput(
                "`offset` is 1-based; pass 1 for the first line or omit it.".to_owned(),
            ));
        }
        let limit = input.limit.unwrap_or(usize::MAX);
        let lines: Vec<&str> = raw
            .lines()
            .enumerate()
            .skip(start - 1)
            .take(limit)
            .map(|(idx, line)| {
                let _ = idx;
                line
            })
            .collect();

        let mut rendered = String::new();
        for (idx, line) in lines.iter().enumerate() {
            if !rendered.is_empty() {
                rendered.push('\n');
            }
            rendered.push_str(&(start + idx).to_string());
            rendered.push('|');
            rendered.push_str(line);
        }
        if rendered.is_empty() && !raw.is_empty() {
            rendered.push_str("[no lines matched the requested offset/limit]");
        }
        if rendered.is_empty() {
            rendered.push_str("[file is empty]");
        }
        if bytes.len() != raw.len() {
            rendered.push_str("\n\n[Note: invalid UTF-8 bytes were replaced for display.]");
        }
        let total_lines = raw.lines().count();
        if start.saturating_sub(1).saturating_add(lines.len()) < total_lines {
            rendered.push_str("\n\n[... file continues; call Read with a later offset ...]");
        }

        let (rendered, truncated) = truncate_chars(&rendered, MAX_READ_CHARS);
        let structured = serde_json::json!({
            "file_path": path,
            "bytes": bytes.len(),
            "total_lines": total_lines,
            "shown_lines": lines.len(),
            "truncated": truncated,
        });
        Ok(ToolOutput {
            content: vec![agentloop_contracts::ToolResultBlock::markdown(rendered)],
            is_error: false,
            structured: Some(structured),
        })
    }
}
